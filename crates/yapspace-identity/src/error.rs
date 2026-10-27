//! Reasons a URL is refused. The strings are part of the API contract (api.md §F.1), so keep them stable.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    Invalid,
    TooLong,
    UnsupportedScheme,
    IpHost,
    PrivateHost,
    BlockedHost,
    SensitiveParameters,
    SecretLikeToken,
}

impl RejectReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::TooLong => "too_long",
            Self::UnsupportedScheme => "unsupported_scheme",
            Self::IpHost => "ip_host",
            Self::PrivateHost => "private_host",
            Self::BlockedHost => "blocked_host",
            Self::SensitiveParameters => "sensitive_parameters",
            Self::SecretLikeToken => "secret_like_token",
        }
    }
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::error::Error for RejectReason {}
