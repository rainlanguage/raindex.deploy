use cynic::{
    serde::{Deserialize, Serialize},
    GraphQlError, GraphQlResponse, QueryBuilder, QueryFragment,
};
use once_cell::sync::OnceCell;
use reqwest::{Client, Url};
use thiserror::Error;

static CYNIC_HTTP_CLIENT: OnceCell<Client> = OnceCell::new();

fn shared_http_client() -> Result<&'static Client, reqwest::Error> {
    CYNIC_HTTP_CLIENT.get_or_try_init(|| Client::builder().build())
}

#[derive(Error, Debug)]
pub enum CynicClientError {
    #[error("Graphql errors: {}", .0.iter().map(|e| e.message.clone()).collect::<Vec<String>>().join(", "))]
    GraphqlError(Vec<GraphQlError>),
    #[error("Subgraph query returned no data")]
    Empty,
    #[error("Request Error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("HTTP {status}: {body}")]
    HttpError { status: u16, body: String },
}

pub trait CynicClient {
    fn get_base_url(&self) -> &Url;

    async fn query<R: QueryFragment + QueryBuilder<V> + for<'a> Deserialize<'a>, V: Serialize>(
        &self,
        variables: V,
    ) -> Result<R, CynicClientError> {
        let request_body = R::build(variables);

        let response = shared_http_client()?
            .post(self.get_base_url().clone())
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(CynicClientError::HttpError {
                status: status.as_u16(),
                body,
            });
        }

        let response_deserialized: GraphQlResponse<R> =
            response.json::<GraphQlResponse<R>>().await?;

        match response_deserialized.errors {
            Some(errors) => Err(CynicClientError::GraphqlError(errors)),
            None => response_deserialized.data.ok_or(CynicClientError::Empty),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_the_process_wide_http_client() {
        let first = shared_http_client().expect("build shared HTTP client");
        let second = shared_http_client().expect("reuse shared HTTP client");

        assert!(std::ptr::eq(first, second));
    }
}
