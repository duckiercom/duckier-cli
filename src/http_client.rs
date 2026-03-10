use anyhow::{anyhow, Context, Result};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HeaderMap, HeaderValue, LOCATION};
use hyper::{Method, Request, StatusCode, Uri};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::de::DeserializeOwned;

type RequestBody = Full<Bytes>;
type HttpsClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, RequestBody>;

#[derive(Clone)]
pub struct HttpClient {
    client: HttpsClient,
}

pub struct HttpResponse {
    status: StatusCode,
    body: Bytes,
}

impl HttpResponse {
    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn text(&self) -> Result<String> {
        std::str::from_utf8(&self.body)
            .map(|text| text.to_string())
            .context("response body was not valid UTF-8")
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.body).context("failed to parse JSON response body")
    }

    pub fn bytes(&self) -> &Bytes {
        &self.body
    }
}

impl HttpClient {
    pub fn new() -> Self {
        let connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();

        let client = Client::builder(TokioExecutor::new()).build(connector);
        Self { client }
    }

    pub async fn get(&self, url: &str, headers: HeaderMap<HeaderValue>) -> Result<HttpResponse> {
        self.send(Method::GET, url, headers, Bytes::new()).await
    }

    pub async fn post_json<T: serde::Serialize>(
        &self,
        url: &str,
        mut headers: HeaderMap<HeaderValue>,
        body: &T,
    ) -> Result<HttpResponse> {
        if !headers.contains_key(hyper::header::CONTENT_TYPE) {
            headers.insert(
                hyper::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
        }

        let body = serde_json::to_vec(body)
            .with_context(|| format!("failed to serialize POST {}", url))?;
        self.send(Method::POST, url, headers, Bytes::from(body))
            .await
    }

    async fn send(
        &self,
        method: Method,
        url: &str,
        headers: HeaderMap<HeaderValue>,
        body: Bytes,
    ) -> Result<HttpResponse> {
        let mut current_url = url.to_string();
        let mut redirects = 0usize;

        loop {
            let uri: Uri = current_url
                .parse()
                .with_context(|| format!("invalid URL {}", current_url))?;
            let request = build_request(&method, uri, &headers, body.clone())
                .with_context(|| format!("failed to build {} {}", method, current_url))?;

            let response = self
                .client
                .request(request)
                .await
                .with_context(|| format!("failed to send {} {}", method, current_url))?;

            if method == Method::GET && response.status().is_redirection() {
                if redirects >= 5 {
                    return Err(anyhow!("too many redirects while requesting {}", url));
                }

                if let Some(location) = response.headers().get(LOCATION) {
                    let location = location
                        .to_str()
                        .context("redirect location was not valid ASCII/UTF-8")?;
                    current_url = resolve_redirect(&current_url, location)?;
                    redirects += 1;
                    continue;
                }
            }

            return response_to_http_response(response)
                .await
                .with_context(|| format!("failed to read response from {}", current_url));
        }
    }
}

fn build_request(
    method: &Method,
    uri: Uri,
    headers: &HeaderMap<HeaderValue>,
    body: Bytes,
) -> Result<Request<RequestBody>, hyper::http::Error> {
    let mut request = Request::builder()
        .method(method.clone())
        .uri(uri)
        .body(Full::new(body))?;
    *request.headers_mut() = headers.clone();
    Ok(request)
}

async fn response_to_http_response(response: hyper::Response<Incoming>) -> Result<HttpResponse> {
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .context("failed to collect response body")?
        .to_bytes();

    Ok(HttpResponse { status, body })
}

fn resolve_redirect(current_url: &str, location: &str) -> Result<String> {
    if location.starts_with("https://") || location.starts_with("http://") {
        return Ok(location.to_string());
    }

    let current: Uri = current_url
        .parse()
        .with_context(|| format!("invalid redirect base URL {}", current_url))?;
    let scheme = current
        .scheme_str()
        .ok_or_else(|| anyhow!("redirect base URL {} had no scheme", current_url))?;

    if location.starts_with("//") {
        return Ok(format!("{}:{}", scheme, location));
    }

    let authority = current
        .authority()
        .map(|value| value.as_str())
        .ok_or_else(|| anyhow!("redirect base URL {} had no authority", current_url))?;

    if location.starts_with('/') {
        return Ok(format!("{}://{}{}", scheme, authority, location));
    }

    let path = current.path();
    let prefix = match path.rsplit_once('/') {
        Some((prefix, _)) if !prefix.is_empty() => prefix,
        _ => "",
    };

    let base = if prefix.is_empty() {
        format!("{}://{}", scheme, authority)
    } else {
        format!("{}://{}{}", scheme, authority, prefix)
    };

    Ok(format!("{}/{}", base, location))
}
