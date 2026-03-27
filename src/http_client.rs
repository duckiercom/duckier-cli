use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::io::Read;

#[derive(Clone)]
pub struct HttpClient {
    agent: ureq::Agent,
}

pub struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

impl HttpResponse {
    pub fn status_code(&self) -> u16 {
        self.status
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn text(&self) -> Result<String> {
        String::from_utf8(self.body.clone()).context("response body was not valid UTF-8")
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.body).context("failed to parse JSON response body")
    }

    pub fn bytes(&self) -> &[u8] {
        &self.body
    }
}

impl HttpClient {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(10))
            .timeout_read(std::time::Duration::from_secs(30))
            .redirects(5)
            .build();
        Self { agent }
    }

    pub fn get(&self, url: &str, headers: &HashMap<String, String>) -> Result<HttpResponse> {
        let mut request = self.agent.get(url);
        for (key, value) in headers {
            request = request.set(key, value);
        }

        match request.call() {
            Ok(response) => read_response(response),
            Err(ureq::Error::Status(status, response)) => {
                read_response_with_status(status, response)
            }
            Err(e) => Err(anyhow!("HTTP GET {} failed: {}", url, e)),
        }
    }

    pub fn post_json<T: serde::Serialize>(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        body: &T,
    ) -> Result<HttpResponse> {
        let mut request = self.agent.post(url);
        for (key, value) in headers {
            request = request.set(key, value);
        }
        if !headers.contains_key("content-type") {
            request = request.set("Content-Type", "application/json");
        }

        let body_bytes = serde_json::to_vec(body)
            .with_context(|| format!("failed to serialize POST {}", url))?;

        match request.send_bytes(&body_bytes) {
            Ok(response) => read_response(response),
            Err(ureq::Error::Status(status, response)) => {
                read_response_with_status(status, response)
            }
            Err(e) => Err(anyhow!("HTTP POST {} failed: {}", url, e)),
        }
    }
}

fn read_response(response: ureq::Response) -> Result<HttpResponse> {
    let status = response.status();
    let mut body = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut body)
        .context("failed to read response body")?;
    Ok(HttpResponse { status, body })
}

fn read_response_with_status(status: u16, response: ureq::Response) -> Result<HttpResponse> {
    let mut body = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut body)
        .context("failed to read error response body")?;
    Ok(HttpResponse { status, body })
}
