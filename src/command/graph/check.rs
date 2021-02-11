use core::num;

use prettytable::{cell, row, Table};
use serde::Serialize;
use structopt::StructOpt;

use rover_client::query::graph::check;

use crate::command::{
    output::{Change, ChangeSeverity},
    RoverStdout,
};
use crate::utils::client::StudioClientConfig;
use crate::utils::git::GitContext;
use crate::utils::loaders::load_schema_from_flag;
use crate::utils::parsers::{
    parse_graph_ref, parse_query_count_threshold, parse_query_percentage_threshold,
    parse_schema_source, parse_validation_period, GraphRef, SchemaSource, ValidationPeriod,
};
use crate::{anyhow, Result};

#[derive(Debug, Serialize, StructOpt)]
pub struct Check {
    /// <NAME>@<VARIANT> of graph in Apollo Studio to validate.
    /// @<VARIANT> may be left off, defaulting to @current
    #[structopt(name = "GRAPH_REF", parse(try_from_str = parse_graph_ref))]
    #[serde(skip_serializing)]
    graph: GraphRef,

    /// Name of configuration profile to use
    #[structopt(long = "profile", default_value = "default")]
    #[serde(skip_serializing)]
    profile_name: String,

    /// The schema file to push
    /// Can pass `-` to use stdin instead of a file
    #[structopt(long, short = "s", parse(try_from_str = parse_schema_source))]
    #[serde(skip_serializing)]
    schema: SchemaSource,

    /// The minimum number of times a query or mutation must have been executed
    /// in order to be considered in the check operation
    #[structopt(long, parse(try_from_str = parse_query_count_threshold))]
    query_count_threshold: Option<i64>,

    /// Minimum percentage of times a query or mutation must have been executed
    /// in the time window, relative to total request count, for it to be
    /// considered in the check. Valid numbers are in the range 0 <= x <= 100
    #[structopt(long, parse(try_from_str = parse_query_percentage_threshold))]
    query_percentage_threshold: Option<f64>,

    /// Size of the time window with which to validate schema against (i.e "24h" or "1w 2d 5h")
    #[structopt(long, parse(try_from_str = parse_validation_period))]
    validation_period: Option<ValidationPeriod>,
}

impl Check {
    pub fn run(
        &self,
        client_config: StudioClientConfig,
        git_context: GitContext,
    ) -> Result<RoverStdout> {
        let client = client_config.get_client(&self.profile_name)?;
        let sdl = load_schema_from_flag(&self.schema, std::io::stdin())?;
        let res = check::run(
            check::check_schema_query::Variables {
                graph_id: self.graph.name.clone(),
                variant: Some(self.graph.variant.clone()),
                schema: Some(sdl),
                git_context: git_context.into(),
                config: check::check_schema_query::HistoricQueryParameters {
                    query_count_threshold: self.query_count_threshold,
                    query_count_threshold_percentage: self.query_percentage_threshold,
                    from: self.validation_period.clone().unwrap_or_default().from,
                    to: self.validation_period.clone().unwrap_or_default().to,
                    // we don't support configuring these, but we can't leave them out
                    excluded_clients: None,
                    ignored_operations: None,
                    included_variants: None,
                },
            },
            &client,
        )?;

        tracing::info!(
            "Validated the proposed graph against metrics from {}",
            &self.graph
        );

        let num_changes = res.changes.len();

        let msg = match num_changes {
            0 => "There is no difference between the proposed graph and the graph that already exists in the graph registry. Try making a change to your proposed graph before running this command.".to_string(),
            _ => format!("Compared {} schema changes against {} operations", res.changes.len(), res.number_of_checked_operations),
        };

        tracing::info!("{}", &msg);

        let (changes, num_failures) = get_changes(&res.changes);

        match num_failures {
            0 => Ok(RoverStdout::Changes {
                changes,
                url: res.target_url,
            }),
            1 => Err(anyhow!("Encountered 1 failure.").into()),
            _ => Err(anyhow!("Encountered {} failures.", num_failures).into()),
        }
    }
}

fn get_changes(
    checks: &[check::check_schema_query::CheckSchemaQueryServiceCheckSchemaDiffToPreviousChanges],
) -> (Vec<Change>, i64) {
    let mut changes = Vec::new();
    let mut num_failures = 0;

    if !checks.is_empty() {
        for check in checks {
            let change_severity = match check.severity {
                check::check_schema_query::ChangeSeverity::NOTICE => ChangeSeverity::Pass,
                check::check_schema_query::ChangeSeverity::FAILURE => {
                    num_failures += 1;
                    ChangeSeverity::Fail
                }
                _ => unreachable!("Unknown change severity"),
            };
            changes.push(Change {
                change_severity,
                code: check.code.clone(),
                description: check.description.clone(),
            });
        }
    }

    (changes, num_failures)
}
