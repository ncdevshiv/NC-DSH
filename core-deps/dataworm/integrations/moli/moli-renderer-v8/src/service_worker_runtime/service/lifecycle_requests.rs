use super::*;

impl ServiceWorkerRuntimeService {
    pub(super) fn finish_worker_skip_waiting_requested(
        &self,
        registration_id: ServiceWorkerRegistrationId,
        version_id: ServiceWorkerVersionId,
    ) {
        let progress = {
            let mut state = self.inner.state.lock();
            let Some(version) = state.versions.get_mut(&version_id) else {
                return;
            };
            if version.registration_id != registration_id {
                return;
            }
            version.skip_waiting_requested = true;
            let Some(registration) = state.registrations.get(&registration_id) else {
                return;
            };
            if registration.waiting_version_id != Some(version_id) {
                return;
            }
            self.activation_progress_for_registration_if_ready_locked(&mut state, registration_id)
        };
        for progress in progress {
            self.run_lifecycle_progress(progress);
        }
    }

    pub(super) fn finish_worker_clients_claim_requested(
        &self,
        registration_id: ServiceWorkerRegistrationId,
        version_id: ServiceWorkerVersionId,
    ) {
        let progress = {
            let mut state = self.inner.state.lock();
            let Some(version) = state.versions.get(&version_id) else {
                return;
            };
            if version.registration_id != registration_id {
                return;
            }
            let Some(registration) = state.registrations.get(&registration_id) else {
                return;
            };
            if registration.active_version_id == Some(version_id) {
                let claim_result = claim_live_scope_clients_locked(&mut state, registration_id);
                let mut progress = claim_result
                    .controller_change_deliveries
                    .into_iter()
                    .map(LifecycleProgress::NotifyControllerChange)
                    .collect::<Vec<_>>();
                progress.extend(claim_result.changed_registration_ids.into_iter().flat_map(
                    |changed_registration_id| {
                        if state
                            .registrations
                            .get(&changed_registration_id)
                            .is_some_and(|registration| registration.pending_unregistration)
                        {
                            self.unregistration_progress_for_registration_if_ready_locked(
                                &mut state,
                                changed_registration_id,
                            )
                        } else {
                            self.activation_progress_for_registration_if_ready_locked(
                                &mut state,
                                changed_registration_id,
                            )
                        }
                    },
                ));
                progress
            } else {
                let should_defer_claim = registration.waiting_version_id == Some(version_id)
                    && version.lifecycle_state == ServiceWorkerVersionLifecycleState::Activating;
                if should_defer_claim && let Some(version) = state.versions.get_mut(&version_id) {
                    version.clients_claim_requested = true;
                }
                Vec::new()
            }
        };
        for progress in progress {
            self.run_lifecycle_progress(progress);
        }
    }
}
