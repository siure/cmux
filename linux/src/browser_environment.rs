use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BrowserEnvironmentState {
    pub locale: String,
    pub timezone: String,
    pub media_type: String,
    pub color_scheme: String,
    pub reduced_motion: String,
    pub offline: bool,
    pub geolocation: Option<BrowserGeolocationState>,
    pub mobile: bool,
    pub touch: bool,
    pub device_scale_factor: f64,
    pub permissions: BTreeMap<String, String>,
}

impl Default for BrowserEnvironmentState {
    fn default() -> Self {
        Self {
            locale: "en-US".to_string(),
            timezone: "UTC".to_string(),
            media_type: "screen".to_string(),
            color_scheme: "light".to_string(),
            reduced_motion: "no-preference".to_string(),
            offline: false,
            geolocation: None,
            mobile: false,
            touch: false,
            device_scale_factor: 1.0,
            permissions: BTreeMap::new(),
        }
    }
}

impl BrowserEnvironmentState {
    #[allow(dead_code)]
    pub(crate) fn from_snapshot(value: Option<&Value>) -> Self {
        value
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    #[cfg_attr(not(feature = "gtk"), allow(dead_code))]
    pub(crate) fn bootstrap_script(&self) -> Result<String, String> {
        let payload = serde_json::to_string(self)
            .map_err(|err| format!("serialize browser environment: {err}"))?;
        Ok(format!("{BROWSER_ENVIRONMENT_BOOTSTRAP}({payload});"))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct BrowserGeolocationState {
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy: f64,
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
const BROWSER_ENVIRONMENT_BOOTSTRAP: &str = r#"(function(env) {
    'use strict';
    var originalsKey = Symbol.for('cmux.browser.environment.originals');
    var activeKey = Symbol.for('cmux.browser.environment.active');
    var originals = window[originalsKey];
    if (!originals) {
        originals = {
            language: navigator.language,
            languages: Array.from(navigator.languages || []),
            online: navigator.onLine,
            maxTouchPoints: Number(navigator.maxTouchPoints || 0),
            geolocation: navigator.geolocation,
            permissionsQuery: navigator.permissions && typeof navigator.permissions.query === 'function'
                ? navigator.permissions.query.bind(navigator.permissions)
                : null,
            matchMedia: typeof window.matchMedia === 'function' ? window.matchMedia.bind(window) : null,
            dateTimeFormat: Intl.DateTimeFormat,
            devicePixelRatio: Number(window.devicePixelRatio || 1)
        };
        Object.defineProperty(window, originalsKey, { value: originals, configurable: false });
    }

    var previous = window[activeKey];
    Object.defineProperty(window, activeKey, { value: env, configurable: true });
    var getter = function(object, name, value) {
        try {
            Object.defineProperty(object, name, {
                configurable: true,
                enumerable: true,
                get: function() { return value; }
            });
        } catch (_) {}
    };

    getter(navigator, 'language', String(env.locale || originals.language));
    getter(navigator, 'languages', [String(env.locale || originals.language)]);
    getter(navigator, 'onLine', !Boolean(env.offline));
    getter(navigator, 'maxTouchPoints', env.touch ? Math.max(1, originals.maxTouchPoints) : 0);
    getter(window, 'devicePixelRatio', Number(env.device_scale_factor || originals.devicePixelRatio));
    getter(window, '__cmuxMobile', Boolean(env.mobile));
    if (env.touch) {
        getter(window, 'ontouchstart', null);
    } else {
        try { delete window.ontouchstart; } catch (_) {}
    }

    var NativeDateTimeFormat = originals.dateTimeFormat;
    var CmuxDateTimeFormat = function(locales, options) {
        var selectedLocales = locales == null ? String(env.locale || originals.language) : locales;
        var selectedOptions = Object.assign({}, options || {});
        if (!selectedOptions.timeZone && env.timezone) selectedOptions.timeZone = String(env.timezone);
        return new NativeDateTimeFormat(selectedLocales, selectedOptions);
    };
    CmuxDateTimeFormat.prototype = NativeDateTimeFormat.prototype;
    Object.setPrototypeOf(CmuxDateTimeFormat, NativeDateTimeFormat);
    CmuxDateTimeFormat.supportedLocalesOf = NativeDateTimeFormat.supportedLocalesOf.bind(NativeDateTimeFormat);
    try { Intl.DateTimeFormat = CmuxDateTimeFormat; } catch (_) {}

    var forcedMediaMatch = function(query) {
        var normalized = String(query || '').toLowerCase();
        var checks = [];
        var color = normalized.match(/prefers-color-scheme\s*:\s*(dark|light)/);
        if (color) checks.push(color[1] === String(env.color_scheme || 'light').toLowerCase());
        var motion = normalized.match(/prefers-reduced-motion\s*:\s*(reduce|no-preference)/);
        if (motion) checks.push(motion[1] === String(env.reduced_motion || 'no-preference').toLowerCase());
        var media = normalized.match(/^(?:only\s+)?(screen|print)(?:\s+and\s+|$)/);
        if (media) checks.push(media[1] === String(env.media_type || 'screen').toLowerCase());
        return checks.length ? checks.every(Boolean) : null;
    };
    if (originals.matchMedia) {
        window.matchMedia = function(query) {
            var nativeResult = originals.matchMedia(query);
            var forced = forcedMediaMatch(query);
            if (forced === null) return nativeResult;
            return {
                matches: forced,
                media: nativeResult.media,
                onchange: null,
                addListener: nativeResult.addListener ? nativeResult.addListener.bind(nativeResult) : function() {},
                removeListener: nativeResult.removeListener ? nativeResult.removeListener.bind(nativeResult) : function() {},
                addEventListener: nativeResult.addEventListener ? nativeResult.addEventListener.bind(nativeResult) : function() {},
                removeEventListener: nativeResult.removeEventListener ? nativeResult.removeEventListener.bind(nativeResult) : function() {},
                dispatchEvent: nativeResult.dispatchEvent ? nativeResult.dispatchEvent.bind(nativeResult) : function() { return false; }
            };
        };
    }
    try { document.documentElement.style.colorScheme = String(env.color_scheme || 'light'); } catch (_) {}

    if (env.geolocation) {
        var nextWatchId = 1;
        var position = function() {
            return {
                coords: {
                    latitude: Number(env.geolocation.latitude),
                    longitude: Number(env.geolocation.longitude),
                    accuracy: Number(env.geolocation.accuracy || 0),
                    altitude: null,
                    altitudeAccuracy: null,
                    heading: null,
                    speed: null
                },
                timestamp: Date.now()
            };
        };
        getter(navigator, 'geolocation', {
            getCurrentPosition: function(success) {
                if (typeof success === 'function') queueMicrotask(function() { success(position()); });
            },
            watchPosition: function(success) {
                var id = nextWatchId++;
                if (typeof success === 'function') queueMicrotask(function() { success(position()); });
                return id;
            },
            clearWatch: function() {}
        });
    } else {
        getter(navigator, 'geolocation', originals.geolocation);
    }

    if (navigator.permissions) {
        var queryPermission = function(descriptor) {
            var name = descriptor && descriptor.name ? String(descriptor.name) : '';
            var state = env.permissions && env.permissions[name];
            if (state) {
                return Promise.resolve({
                    state: String(state),
                    onchange: null,
                    addEventListener: function() {},
                    removeEventListener: function() {},
                    dispatchEvent: function() { return false; }
                });
            }
            return originals.permissionsQuery
                ? originals.permissionsQuery(descriptor)
                : Promise.reject(new TypeError('Permission query is unavailable'));
        };
        try {
            Object.defineProperty(navigator.permissions, 'query', {
                configurable: true,
                value: queryPermission
            });
        } catch (_) {}
    }

    if (previous && Boolean(previous.offline) !== Boolean(env.offline)) {
        try { window.dispatchEvent(new Event(env.offline ? 'offline' : 'online')); } catch (_) {}
    }
    return true;
})"#;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn browser_environment_snapshot_is_typed_and_defaults_missing_fields() {
        let state = BrowserEnvironmentState::from_snapshot(Some(&json!({
            "locale": "nl-NL",
            "offline": true,
            "geolocation": {
                "latitude": 52.37,
                "longitude": 4.90,
                "accuracy": 8.0
            },
            "permissions": {"geolocation": "granted"}
        })));

        assert_eq!(state.locale, "nl-NL");
        assert_eq!(state.timezone, "UTC");
        assert!(state.offline);
        assert_eq!(state.geolocation.expect("geolocation").accuracy, 8.0);
        assert_eq!(
            state.permissions.get("geolocation").map(String::as_str),
            Some("granted")
        );
        assert_eq!(
            BrowserEnvironmentState::from_snapshot(None),
            BrowserEnvironmentState::default()
        );
    }

    #[test]
    fn browser_environment_bootstrap_serializes_values_as_json() {
        let state = BrowserEnvironmentState {
            locale: "en-GB'; window.bad = true; //".to_string(),
            timezone: "Europe/London".to_string(),
            offline: true,
            ..BrowserEnvironmentState::default()
        };
        let script = state.bootstrap_script().expect("bootstrap script");

        assert!(script.starts_with("(function(env)"));
        assert!(script.contains(r#""locale":"en-GB'; window.bad = true; //""#));
        assert!(script.contains(r#""offline":true"#));
        assert!(script.ends_with(");"));
    }
}
