use moli_shared_worker::SharedWorkerInstanceId;

pub(super) fn shared_worker_thread_name(kind: &str, instance_id: SharedWorkerInstanceId) -> String {
    let mut name = format!("{kind}:{}", instance_id.as_u64());
    name.truncate(15);
    name
}

#[cfg(test)]
mod tests {
    use moli_shared_worker::SharedWorkerInstanceId;

    use super::shared_worker_thread_name;

    #[test]
    fn shared_worker_thread_names_fit_linux_limit() {
        let instance_id = SharedWorkerInstanceId::from_u64(u64::MAX);

        assert_eq!(shared_worker_thread_name("sw-load", instance_id).len(), 15);
        assert_eq!(shared_worker_thread_name("sw-pump", instance_id).len(), 15);
    }
}
