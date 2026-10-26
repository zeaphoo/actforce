//! Generic entitlement model shared by ActForce products.
//!
//! This crate intentionally knows nothing about licenses, RSA keys, hardware
//! binding, or activation. It only describes the *result* of an authorization
//! decision and the provider interface that produces it.
//!
//! Public ActForce uses [`CommunityProvider`]. Private ActForce-Super supplies
//! an implementation backed by its own license domain.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;

/// High-level authorization state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntitlementStatus {
    /// Open-source/community build: all public capabilities are available.
    Community,
    /// A valid license is active.
    Active,
    /// The license has expired but is still inside its grace period.
    Grace,
    /// The license has expired and the grace period is over.
    Expired,
    /// A license exists but failed validation (signature, hardware, format).
    Invalid,
    /// No license has been activated yet.
    Unlicensed,
}

/// A named numeric limit, e.g. `users.max`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limit {
    pub max: Option<u64>,
    pub used: Option<u64>,
}

/// Normalized authorization result. Frontend and backend both consume this
/// shape; neither should need to know how it was produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entitlements {
    pub tier: String,
    pub status: EntitlementStatus,
    pub features: BTreeSet<String>,
    pub limits: BTreeMap<String, Limit>,
    pub expires_at: Option<DateTime<Utc>>,
    pub grace_until: Option<DateTime<Utc>>,
    pub messages: Vec<String>,
}

impl Entitlements {
    /// Community build: no license required, all public capabilities enabled.
    pub fn community() -> Self {
        Self {
            tier: "community".to_string(),
            status: EntitlementStatus::Community,
            features: COMMUNITY_FEATURES
                .iter()
                .map(|f| (*f).to_string())
                .collect(),
            limits: BTreeMap::new(),
            expires_at: None,
            grace_until: None,
            messages: Vec::new(),
        }
    }

    /// Returns true when protected API access should be denied.
    pub fn is_blocked(&self) -> bool {
        matches!(
            self.status,
            EntitlementStatus::Expired | EntitlementStatus::Invalid | EntitlementStatus::Unlicensed
        )
    }

    /// Feature check used by handlers.
    ///
    /// The community build is intentionally unrestricted; gating is a private
    /// enterprise concern. Licensed builds use the feature set produced by
    /// their provider.
    pub fn has_feature(&self, feature: &str) -> bool {
        if self.status == EntitlementStatus::Community {
            return true;
        }
        self.features.contains(feature)
    }

    pub fn limit(&self, key: &str) -> Option<&Limit> {
        self.limits.get(key)
    }
}

/// Public feature keys known to the community build. Private products may add
/// their own string keys without changing this list.
pub const COMMUNITY_FEATURES: &[&str] = &[
    "app.access",
    "chat.basic",
    "chat.attachments",
    "file.manage",
    "file.upload",
    "file.search",
    "skills.manage",
    "skills.custom",
    "quick_prompts",
    "system.metrics",
    "disks",
    "export.markdown",
    "aupage",
    "knowledge",
    "inference.local",
    "admin.users",
    "admin.skills",
    "admin.settings",
    "admin.quick_prompts",
    "admin.system",
];

#[derive(Debug, Error)]
pub enum EntitlementError {
    #[error("entitlement provider error: {0}")]
    Provider(String),
    #[error("feature not entitled: {0}")]
    FeatureNotEntitled(String),
    #[error("limit exceeded: {key} (max {max}, used {used})")]
    LimitExceeded { key: String, max: u64, used: u64 },
}

/// Produces the current authorization result. Implementations may read a DB,
/// verify a signed license, call a license server, etc.
#[async_trait]
pub trait EntitlementProvider: Send + Sync {
    async fn entitlements(&self) -> Entitlements;

    /// Re-read the provider's source of truth. The default is a no-op.
    async fn refresh(&self) -> Result<(), EntitlementError> {
        Ok(())
    }
}

/// Default provider for the open-source community build.
pub struct CommunityProvider;

#[async_trait]
impl EntitlementProvider for CommunityProvider {
    async fn entitlements(&self) -> Entitlements {
        Entitlements::community()
    }
}

/// Caches the current entitlements and exposes the checks handlers need.
pub struct EntitlementService {
    provider: Arc<dyn EntitlementProvider>,
    current: RwLock<Entitlements>,
}

impl EntitlementService {
    pub async fn new(provider: Arc<dyn EntitlementProvider>) -> Result<Self, EntitlementError> {
        let current = provider.entitlements().await;
        Ok(Self {
            provider,
            current: RwLock::new(current),
        })
    }

    pub async fn current(&self) -> Entitlements {
        self.current.read().await.clone()
    }

    pub async fn refresh(&self) -> Result<(), EntitlementError> {
        self.provider.refresh().await?;
        let next = self.provider.entitlements().await;
        *self.current.write().await = next;
        Ok(())
    }

    pub async fn has_feature(&self, feature: &str) -> bool {
        self.current.read().await.has_feature(feature)
    }

    pub async fn is_blocked(&self) -> bool {
        self.current.read().await.is_blocked()
    }

    /// Checks `used < max` for a named limit. Unknown limits are unrestricted.
    pub async fn check_limit(&self, key: &str, used: u64) -> Result<(), EntitlementError> {
        let current = self.current.read().await;
        if let Some(limit) = current.limit(key) {
            if let Some(max) = limit.max {
                if used >= max {
                    return Err(EntitlementError::LimitExceeded {
                        key: key.to_string(),
                        max,
                        used,
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_is_open() {
        let entitlements = Entitlements::community();
        assert!(!entitlements.is_blocked());
        assert!(entitlements.has_feature("anything"));
        assert_eq!(entitlements.tier, "community");
    }

    #[tokio::test]
    async fn service_loads_community_provider() {
        let service = EntitlementService::new(Arc::new(CommunityProvider))
            .await
            .expect("community provider cannot fail");
        assert!(service.has_feature("file.manage").await);
        assert!(!service.is_blocked().await);
        assert!(service.check_limit("users.max", 100).await.is_ok());
    }
}
