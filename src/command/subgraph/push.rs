use serde::Serialize;
use structopt::StructOpt;

use crate::client::StudioClientConfig;
use crate::command::RoverStdout;
use crate::utils::loaders::load_schema_from_flag;
use crate::utils::parsers::{parse_graph_ref, parse_schema_source, GraphRef, SchemaSource};
use crate::{Context, Result};

use rover_client::query::subgraph::push::{self, PushPartialSchemaResponse};

#[derive(Debug, Serialize, StructOpt)]
pub struct Push {
    /// <NAME>@<VARIANT> of federated graph in Apollo Studio to push to.
    /// @<VARIANT> may be left off, defaulting to @current
    #[structopt(name = "GRAPH_REF", parse(try_from_str = parse_graph_ref))]
    #[serde(skip_serializing)]
    graph: GraphRef,

    /// The schema file to push
    /// Can pass `-` to use stdin instead of a file
    #[structopt(long, short = "s", parse(try_from_str = parse_schema_source))]
    #[serde(skip_serializing)]
    schema: SchemaSource,

    /// Name of configuration profile to use
    #[structopt(long = "profile", default_value = "default")]
    #[serde(skip_serializing)]
    profile_name: String,

    /// Name of subgraph in federated graph to update
    #[structopt(long = "name")]
    #[serde(skip_serializing)]
    subgraph: String,

    /// Url of a running subgraph that a gateway can route operations to
    /// (often a deployed subgraph). May be left empty ("") or a placeholder url
    /// if not running a gateway in managed federation mode
    #[structopt(long)]
    routing_url: String,
}

impl Push {
    pub fn run(&self, client_config: StudioClientConfig) -> Result<RoverStdout> {
        let client = client_config.get_client(&self.profile_name)?;
        tracing::info!(
            "Pushing the {} subgraph to {}@{}, mx. {}!",
            &self.subgraph,
            &self.graph.name,
            &self.graph.variant,
            &self.profile_name
        );

        let schema_document = load_schema_from_flag(&self.schema, std::io::stdin())?;

        tracing::debug!("Schema Document to push:\n{}", &schema_document);

        let push_response = push::run(
            push::push_partial_schema_mutation::Variables {
                id: self.graph.name.clone(),
                graph_variant: self.graph.variant.clone(),
                name: self.subgraph.clone(),
                active_partial_schema: push::push_partial_schema_mutation::PartialSchemaInput {
                    sdl: Some(schema_document),
                    hash: None,
                },
                revision: "".to_string(),
                url: self.routing_url.clone(),
            },
            &client,
        )
        .context("Failed while pushing to Apollo Studio. To see a full printout of the schema attempting to push, rerun with `--log debug`")?;

        handle_response(push_response, &self.subgraph, &self.graph.name);
        Ok(RoverStdout::None)
    }
}

fn handle_response(response: PushPartialSchemaResponse, subgraph: &str, graph: &str) {
    if response.service_was_created {
        tracing::info!(
            "A new subgraph called '{}' for the '{}' graph was created",
            subgraph,
            graph
        );
    } else {
        tracing::info!(
            "The '{}' subgraph for the '{}' graph was updated",
            subgraph,
            graph
        );
    }

    if response.did_update_gateway {
        tracing::info!("The gateway for the '{}' graph was updated with a new schema, composed from the updated '{}' subgraph", graph, subgraph);
    } else {
        tracing::info!(
            "The gateway for the '{}' graph was NOT updated with a new schema",
            graph
        );
    }

    if let Some(errors) = response.composition_errors {
        tracing::error!(
            "The following composition errors occurred: \n{}",
            errors.join("\n")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{handle_response, PushPartialSchemaResponse};

    // this test is a bit weird, since we can't test the output. We just verify it
    // doesn't error
    #[test]
    fn handle_response_doesnt_error_with_all_successes() {
        let response = PushPartialSchemaResponse {
            schema_hash: Some("123456".to_string()),
            did_update_gateway: true,
            service_was_created: true,
            composition_errors: None,
        };

        handle_response(response, "accounts", "my-graph");
    }

    #[test]
    fn handle_response_doesnt_error_with_all_failures() {
        let response = PushPartialSchemaResponse {
            schema_hash: None,
            did_update_gateway: false,
            service_was_created: false,
            composition_errors: Some(vec![
                "a bad thing happened".to_string(),
                "another bad thing".to_string(),
            ]),
        };

        handle_response(response, "accounts", "my-graph");
    }

    // TODO: test the actual output of the logs whenever we do design work
    // for the commands :)
}