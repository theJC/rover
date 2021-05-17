use crate::blocking::StudioClient;
use crate::RoverClientError;
use graphql_client::*;

use reqwest::Url;

type Timestamp = String;
#[derive(GraphQLQuery)]
// The paths are relative to the directory where your `Cargo.toml` is located.
// Both json and the GraphQL schema language are supported as sources for the schema
#[graphql(
    query_path = "src/query/subgraph/check.graphql",
    schema_path = ".schema/schema.graphql",
    response_derives = "PartialEq, Debug, Serialize, Deserialize",
    deprecated = "warn"
)]
/// This struct is used to generate the module containing `Variables` and
/// `ResponseData` structs.
/// Snake case of this name is the mod name. i.e. check_partial_schema_mutation
pub struct CheckPartialSchemaMutation;

/// The main function to be used from this module.
/// This function takes a proposed schema and validates it against a published
/// schema.
pub fn run(
    variables: check_partial_schema_mutation::Variables,
    client: &StudioClient,
) -> Result<CheckResponse, RoverClientError> {
    let graph = variables.graph_id.clone();
    let invalid_variant = variables.variant.clone();
    let data = client.post::<CheckPartialSchemaMutation>(variables)?;
    get_check_response_from_data(data, graph, invalid_variant)
}

pub enum CheckResponse {
    CompositionErrors(Vec<check_partial_schema_mutation::CheckPartialSchemaMutationServiceCheckPartialSchemaCompositionValidationResultErrors>),
    CheckResult(CheckResult)
}

#[derive(Debug)]
pub struct CheckResult {
    pub target_url: Option<Url>,
    pub number_of_checked_operations: i64,
    pub change_severity: check_partial_schema_mutation::ChangeSeverity,
    pub changes: Vec<check_partial_schema_mutation::CheckPartialSchemaMutationServiceCheckPartialSchemaCheckSchemaResultDiffToPreviousChanges>,
}

type ImplementingServices = check_partial_schema_mutation::CheckPartialSchemaMutationServiceServiceImplementingServices;

fn get_check_response_from_data(
    data: check_partial_schema_mutation::ResponseData,
    graph: String,
    invalid_variant: String
) -> Result<CheckResponse, RoverClientError> {
    let service = data.service.ok_or(RoverClientError::NoService { graph: graph.clone() })?;
    
    match service.service.implementing_services {
        Some(typename) => match typename {
            ImplementingServices::FederatedImplementingServices => {
                Ok(())
            }
            ImplementingServices::NonFederatedImplementingService => {
                Err(RoverClientError::ExpectedFederatedGraph { graph })
            }
        },
        None => {
            let mut valid_variants = Vec::new();

            for variant in service.service.variants {
                valid_variants.push(variant.name)
            }
            // TODO: fix front end url root once it's available in mutations
            Err(RoverClientError::NoSchemaForVariant {
                graph,
                invalid_variant,
                valid_variants,
                frontend_url_root: "https://studio.apollographql.com".to_string(),
            })
        },
    }?;

    // for some reason this is a `Vec<Option<CompositionError>>`
    // we convert this to just `Vec<CompositionError>` because the `None`
    // errors would be useless.
    let composition_errors: Vec<check_partial_schema_mutation::CheckPartialSchemaMutationServiceCheckPartialSchemaCompositionValidationResultErrors> = service
        .check_partial_schema
        .composition_validation_result
        .errors;

    if composition_errors.is_empty() {
        let check_schema_result = service.check_partial_schema.check_schema_result.ok_or(
            RoverClientError::MalformedResponse {
                null_field: "service.check_partial_schema.check_schema_result".to_string(),
            },
        )?;

        let target_url = get_url(check_schema_result.target_url);

        let diff_to_previous = check_schema_result.diff_to_previous;

        let number_of_checked_operations =
            diff_to_previous.number_of_checked_operations.unwrap_or(0);

        let change_severity = diff_to_previous.severity;
        let changes = diff_to_previous.changes;

        let check_result = CheckResult {
            target_url,
            number_of_checked_operations,
            change_severity,
            changes,
        };

        Ok(CheckResponse::CheckResult(check_result))
    } else {
        Ok(CheckResponse::CompositionErrors(composition_errors))
    }
}

fn get_url(url: Option<String>) -> Option<Url> {
    match url {
        Some(url) => {
            let url = Url::parse(&url);
            match url {
                Ok(url) => Some(url),
                // if the API returns an invalid URL, don't put it in the response
                Err(_) => None,
            }
        }
        None => None,
    }
}
