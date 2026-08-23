use crate::conn::{EmulatedDeviceMetrics, viewport_surface_install_script};

pub(super) fn live_device_metrics_override_script(
    metrics: &EmulatedDeviceMetrics,
    remember_original_descriptors: bool,
) -> String {
    viewport_surface_install_script(&metrics.viewport_surface(), remember_original_descriptors)
}

pub(super) const LIVE_DEVICE_METRICS_CLEAR_SCRIPT: &str = r#"
(() => {
  const storeKey = '__moliDeviceMetricsOriginalDescriptors';
  const descriptors = globalThis[storeKey] || {};
  let isTopLevelWindow = true;
  try {
    isTopLevelWindow = globalThis.parent === globalThis;
  } catch (_) {
  }
  const restoreDescriptor = (scope, object, property) => {
    if (!object) {
      return;
    }
    const key = `${scope}.${property}`;
    try {
      if (Object.prototype.hasOwnProperty.call(descriptors, key)) {
        const original = descriptors[key];
        if (original && original.existed && original.descriptor) {
          Object.defineProperty(object, property, original.descriptor);
        } else {
          delete object[property];
        }
      } else {
        delete object[property];
      }
    } catch (_) {
    }
  };
  const windowProperties = ['outerWidth', 'outerHeight', 'devicePixelRatio'];
  if (isTopLevelWindow) {
    windowProperties.unshift('innerWidth', 'innerHeight');
  }
  for (const property of windowProperties) {
    restoreDescriptor('window', globalThis, property);
  }
  for (const property of ['width', 'height', 'availWidth', 'availHeight']) {
    restoreDescriptor('screen', globalThis.screen, property);
  }
  try {
    delete globalThis[storeKey];
  } catch (_) {
  }
})()
"#;

#[cfg(test)]
mod tests {
    use super::LIVE_DEVICE_METRICS_CLEAR_SCRIPT;

    #[test]
    fn live_device_metrics_clear_script_uses_plain_helper_store() {
        assert!(!LIVE_DEVICE_METRICS_CLEAR_SCRIPT.contains("Object.create(null)"));
        assert!(LIVE_DEVICE_METRICS_CLEAR_SCRIPT.contains("globalThis.parent === globalThis"));
        assert!(LIVE_DEVICE_METRICS_CLEAR_SCRIPT.contains("if (isTopLevelWindow)"));
    }
}
