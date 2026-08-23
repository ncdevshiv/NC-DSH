use serde::{Deserialize, Serialize};

use moli_cookie_jar::{CookieSiteDataOperation, CookieStorageClearTarget};

use super::{
    BrowserContext, CdpConnection,
    cookie_owner::{
        BrowserContextCookieBoundaryOperationPreviewReport,
        BrowserContextCookieBoundaryOperationReport, BrowserContextCookieBoundarySnapshot,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum BrowserContextSiteDataManagerOwnerState {
    #[default]
    CookieOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum BrowserContextReservedSiteDataOwnerState {
    #[default]
    Reserved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserContextSiteDataManagerSurfaceSnapshot {
    // Today this owner surface is backed only by cookie state, but keep one
    // stable manager shape so future non-cookie site data does not need a new
    // boundary family.
    pub(crate) owner_state: BrowserContextSiteDataManagerOwnerState,
    pub(crate) cookie_boundary: BrowserContextCookieBoundarySnapshot,
    pub(crate) reserved_additional_storage: BrowserContextReservedSiteDataOwnerState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserContextSiteDataManagerOperationPreviewReport {
    pub(crate) current_surface: BrowserContextSiteDataManagerSurfaceSnapshot,
    pub(crate) current_target_surface: BrowserContextSiteDataManagerSurfaceSnapshot,
    pub(crate) cookie_boundary_preview: BrowserContextCookieBoundaryOperationPreviewReport,
    pub(crate) resulting_surface: BrowserContextSiteDataManagerSurfaceSnapshot,
    pub(crate) resulting_target_surface: BrowserContextSiteDataManagerSurfaceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserContextSiteDataManagerOperationReport {
    pub(crate) replaced_surface: BrowserContextSiteDataManagerSurfaceSnapshot,
    pub(crate) replaced_target_surface: BrowserContextSiteDataManagerSurfaceSnapshot,
    pub(crate) cookie_boundary_report: BrowserContextCookieBoundaryOperationReport,
    pub(crate) resulting_surface: BrowserContextSiteDataManagerSurfaceSnapshot,
    pub(crate) resulting_target_surface: BrowserContextSiteDataManagerSurfaceSnapshot,
}

impl BrowserContextSiteDataManagerSurfaceSnapshot {
    fn from_cookie_boundary(
        cookie_boundary: BrowserContextCookieBoundarySnapshot,
    ) -> BrowserContextSiteDataManagerSurfaceSnapshot {
        BrowserContextSiteDataManagerSurfaceSnapshot {
            owner_state: BrowserContextSiteDataManagerOwnerState::CookieOnly,
            cookie_boundary,
            reserved_additional_storage: BrowserContextReservedSiteDataOwnerState::Reserved,
        }
    }
}

impl BrowserContext {
    fn site_data_manager_surface_snapshot_for_target(
        &self,
        target: &CookieStorageClearTarget,
    ) -> BrowserContextSiteDataManagerSurfaceSnapshot {
        match target {
            CookieStorageClearTarget::WholeStore => self.site_data_manager_surface_snapshot(),
            CookieStorageClearTarget::RegistrableSites(sites) => self
                .site_data_manager_surface_snapshot_for_sites(
                    &sites.iter().map(String::as_str).collect::<Vec<_>>(),
                ),
        }
    }

    pub(crate) fn site_data_manager_surface_snapshot(
        &self,
    ) -> BrowserContextSiteDataManagerSurfaceSnapshot {
        BrowserContextSiteDataManagerSurfaceSnapshot::from_cookie_boundary(
            self.cookie_boundary_snapshot(),
        )
    }

    pub(crate) fn site_data_manager_surface_snapshot_for_sites(
        &self,
        sites: &[&str],
    ) -> BrowserContextSiteDataManagerSurfaceSnapshot {
        BrowserContextSiteDataManagerSurfaceSnapshot::from_cookie_boundary(
            self.cookie_boundary_snapshot_for_sites(sites),
        )
    }

    pub(crate) fn preview_site_data_manager_operation(
        &self,
        operation: &CookieSiteDataOperation,
    ) -> Result<BrowserContextSiteDataManagerOperationPreviewReport, String> {
        let target = match operation {
            CookieSiteDataOperation::Clear { target, .. } => target,
        };
        let current_surface = self.site_data_manager_surface_snapshot();
        let current_target_surface = self.site_data_manager_surface_snapshot_for_target(target);
        let cookie_boundary_preview = self.preview_cookie_boundary_operation(operation)?;
        let resulting_surface = BrowserContextSiteDataManagerSurfaceSnapshot::from_cookie_boundary(
            cookie_boundary_preview.resulting_boundary.clone(),
        );
        let resulting_target_surface =
            BrowserContextSiteDataManagerSurfaceSnapshot::from_cookie_boundary(
                cookie_boundary_preview.resulting_target_boundary.clone(),
            );
        Ok(BrowserContextSiteDataManagerOperationPreviewReport {
            current_surface,
            current_target_surface,
            cookie_boundary_preview,
            resulting_surface,
            resulting_target_surface,
        })
    }

    pub(crate) fn apply_site_data_manager_operation(
        &self,
        operation: &CookieSiteDataOperation,
    ) -> Result<BrowserContextSiteDataManagerOperationReport, String> {
        let target = match operation {
            CookieSiteDataOperation::Clear { target, .. } => target,
        };
        let replaced_surface = self.site_data_manager_surface_snapshot();
        let replaced_target_surface = self.site_data_manager_surface_snapshot_for_target(target);
        let cookie_boundary_report = self.apply_cookie_boundary_operation(operation)?;
        let resulting_surface = BrowserContextSiteDataManagerSurfaceSnapshot::from_cookie_boundary(
            cookie_boundary_report.resulting_boundary.clone(),
        );
        let resulting_target_surface =
            BrowserContextSiteDataManagerSurfaceSnapshot::from_cookie_boundary(
                cookie_boundary_report.resulting_target_boundary.clone(),
            );
        Ok(BrowserContextSiteDataManagerOperationReport {
            replaced_surface,
            replaced_target_surface,
            cookie_boundary_report,
            resulting_surface,
            resulting_target_surface,
        })
    }
}

impl CdpConnection {
    pub(crate) fn site_data_manager_surface_snapshot(
        &self,
    ) -> Result<BrowserContextSiteDataManagerSurfaceSnapshot, String> {
        self.browser_context
            .as_ref()
            .map(BrowserContext::site_data_manager_surface_snapshot)
            .ok_or_else(|| "BrowserContextNotLoaded".to_owned())
    }

    pub(crate) fn site_data_manager_surface_snapshot_for_sites(
        &self,
        sites: &[&str],
    ) -> Result<BrowserContextSiteDataManagerSurfaceSnapshot, String> {
        self.browser_context
            .as_ref()
            .map(|bc| bc.site_data_manager_surface_snapshot_for_sites(sites))
            .ok_or_else(|| "BrowserContextNotLoaded".to_owned())
    }

    pub(crate) fn preview_site_data_manager_operation(
        &mut self,
        operation: &CookieSiteDataOperation,
    ) -> Result<BrowserContextSiteDataManagerOperationPreviewReport, String> {
        self.browser_context
            .as_ref()
            .ok_or_else(|| "BrowserContextNotLoaded".to_owned())?
            .preview_site_data_manager_operation(operation)
    }

    pub(crate) fn apply_site_data_manager_operation(
        &mut self,
        operation: &CookieSiteDataOperation,
    ) -> Result<BrowserContextSiteDataManagerOperationReport, String> {
        self.browser_context
            .as_ref()
            .ok_or_else(|| "BrowserContextNotLoaded".to_owned())?
            .apply_site_data_manager_operation(operation)
    }
}
