use std::sync::{Arc, OnceLock};

use rustls::{
    ClientConfig, DigitallySignedStruct, Error as TlsError, RootCertStore, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use tokio::net::TcpStream;
use tokio_tungstenite::MaybeTlsStream;

use crate::ConnectOptions;

pub(crate) async fn wrap_websocket_stream(
    uri: &http::Uri,
    stream: TcpStream,
    context: &ConnectOptions,
) -> Result<MaybeTlsStream<TcpStream>, String> {
    match uri.scheme_str() {
        Some("ws") => Ok(MaybeTlsStream::Plain(stream)),
        Some("wss") => wrap_websocket_tls_stream(uri, stream, context).await,
        Some(scheme) => Err(format!("unsupported WebSocket URL scheme `{scheme}`")),
        None => Err("WebSocket URL is missing scheme".to_owned()),
    }
}

async fn wrap_websocket_tls_stream(
    uri: &http::Uri,
    stream: TcpStream,
    context: &ConnectOptions,
) -> Result<MaybeTlsStream<TcpStream>, String> {
    let host = uri
        .host()
        .ok_or_else(|| "WebSocket URL is missing host".to_owned())?
        .trim_matches(['[', ']'])
        .to_owned();
    let server_name = ServerName::try_from(host)
        .map_err(|_| "WebSocket TLS server name is invalid".to_owned())?;
    let connector = tokio_rustls::TlsConnector::from(websocket_tls_config(context));
    let stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|error| format!("WebSocket TLS handshake failed: {error}"))?;
    Ok(MaybeTlsStream::Rustls(stream))
}

fn websocket_tls_config(context: &ConnectOptions) -> Arc<ClientConfig> {
    if context.tls_verify_host {
        return default_webpki_tls_config();
    }
    disabled_verification_tls_config()
}

fn default_webpki_tls_config() -> Arc<ClientConfig> {
    static DEFAULT_TLS_CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    DEFAULT_TLS_CONFIG
        .get_or_init(|| {
            let mut root_store = RootCertStore::empty();
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            Arc::new(
                ClientConfig::builder()
                    .with_root_certificates(root_store)
                    .with_no_client_auth(),
            )
        })
        .clone()
}

fn disabled_verification_tls_config() -> Arc<ClientConfig> {
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
        .with_no_client_auth();
    Arc::new(config)
}

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    // This verifier is only used when the embedding layer explicitly sets
    // tls_verify_host=false, primarily for local development and tests with
    // self-signed certificates. It disables all certificate and signature
    // verification and must not be enabled for production browsing contexts.
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}
