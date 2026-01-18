# CHANGELOG

## v0.4.0

### Added
- HTTPS / TLS support for sobj-server using axum-server + rustls
- Explicit TLS enablement via `tls_enabled` and `--tls` option
- `/healthz` endpoint returning app name, version, and in-flight count
- QUICKSTART.md with HTTPS + local CA setup guide
- API.md documenting all HTTP endpoints

### Changed
- TLS certificate and key paths are resolved relative to `sobj-server.json`
- CLI CA certificate path (`tls_ca_cert_pem_path`) is resolved relative to `sobj.json`
- TLS trust model follows webpki rules (CA-signed certificates required)
- Documentation reorganized for v0.4

### Fixed
- HTTPS startup panic by explicitly selecting rustls crypto provider
- CLI TLS verification failures caused by improper trust anchor usage

### Notes
- Directly trusting a self-signed leaf certificate is **not supported**
- Use a local CA (CA:TRUE) to sign server certificates for HTTPS

---

## v0.3.1
- axum-server migration groundwork
- `/healthz` endpoint introduced

