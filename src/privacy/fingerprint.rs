use webkit2gtk::{
    UserContentInjectedFrames, UserContentManager, UserContentManagerExt, UserScript,
    UserScriptInjectionTime,
};

use crate::privacy::PRIVACY_FINGERPRINT;

/// Marqueur du script. L'injection reste dans le monde page par défaut :
/// un monde isolé rendrait les getters invisibles au JS du site.
pub const FINGERPRINT_WORLD: &str = "liteweb-privacy";

/// JS anti-fuite généré depuis `PRIVACY_FINGERPRINT`. Aucune randomisation.
pub fn fingerprint_script() -> String {
    let s = PRIVACY_FINGERPRINT;
    let languages = s
        .languages()
        .iter()
        .map(|tag| format!("\"{tag}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"(function () {{
  "use strict";
  // {world}
  function pin(object, property, value) {{
    try {{
      Object.defineProperty(object, property, {{
        get: function () {{ return value; }},
        enumerable: true,
        configurable: false
      }});
    }} catch (error) {{}}
  }}
  var nav = window.navigator;
  pin(nav, "userAgent", "{user_agent}");
  pin(nav, "appVersion", "{app_version}");
  pin(nav, "platform", "{platform}");
  pin(nav, "vendor", "Google Inc.");
  pin(nav, "product", "Gecko");
  pin(nav, "productSub", "20030107");
  pin(nav, "language", "fr-FR");
  pin(nav, "webdriver", false); // navigator.webdriver
  try {{ Object.defineProperty(navigator, "webdriver", {{ get: function () {{ return false; }}, configurable: false }}); }} catch (error) {{}}
  pin(nav, "hardwareConcurrency", {hardware_concurrency});
  pin(nav, "deviceMemory", {device_memory});
  pin(nav, "maxTouchPoints", {max_touch_points});
  pin(nav, "doNotTrack", "{dnt}");
  try {{ Object.defineProperty(nav, "languages", {{ get: function () {{ return Object.freeze([{languages}]); }}, configurable: false }}); }} catch (error) {{}}
  try {{ Object.defineProperty(nav, "plugins", {{ get: function () {{ return Object.freeze([]); }}, configurable: false }}); }} catch (error) {{}}
  try {{ Object.defineProperty(nav, "mimeTypes", {{ get: function () {{ return Object.freeze([]); }}, configurable: false }}); }} catch (error) {{}}
  try {{ nav.getBattery = function () {{ return Promise.reject(new DOMException("NotAllowedError")); }}; }} catch (error) {{}}

  // Permissions : WebKit répond "prompt" là où Chrome répond "denied".
  // Un site qui interroge camera/geolocation/microphone voit la différence.
  try {{
    var query = Permissions.prototype.query;
    Permissions.prototype.query = function (desc) {{
      var name = desc && desc.name;
      if (name === "camera" || name === "microphone" || name === "geolocation" || name === "notifications" || name === "push") {{
        return Promise.resolve({{ state: "denied", onchange: null }});
      }}
      return query.call(this, desc);
    }};
  }} catch (error) {{}}

  var screen = window.screen;
  pin(screen, "width", {screen_width});
  pin(screen, "height", {screen_height});
  pin(screen, "availWidth", {screen_avail_width});
  pin(screen, "availHeight", {screen_avail_height});
  pin(screen, "colorDepth", {color_depth});
  pin(screen, "pixelDepth", {pixel_depth});
  try {{ Object.defineProperty(window, "devicePixelRatio", {{ get: function () {{ return {device_pixel_ratio}; }}, configurable: false }}); }} catch (error) {{}}

  try {{
    Object.defineProperty(Date.prototype, "getTimezoneOffset", {{
      value: function () {{ return {timezone_offset}; }},
      configurable: false
    }});
    var resolvedOptions = Intl.DateTimeFormat.prototype.resolvedOptions;
    Intl.DateTimeFormat.prototype.resolvedOptions = function () {{
      var options = resolvedOptions.call(this);
      options.timeZone = "{time_zone}";
      return options;
    }};
  }} catch (error) {{}}

  // Fontes : une machine Linux a ~134 fontes (Aakar, AnjaliOldLipi, …) à 0,00 %.
  // On ne peut pas cacher les fontes du système, mais on peut faire échouer
  // la sonde JS qui mesure la largeur d'un texte par fonte.
  try {{
    var measure = CanvasRenderingContext2D.prototype.measureText;
    CanvasRenderingContext2D.prototype.measureText = function (text) {{
      var metrics = measure.call(this, text);
      var width = Math.round(String(text).length * 7);
      return {{
        width: width,
        actualBoundingBoxLeft: 0,
        actualBoundingBoxRight: width,
        actualBoundingBoxAscent: 8,
        actualBoundingBoxDescent: 2,
        fontBoundingBoxAscent: 10,
        fontBoundingBoxDescent: 3,
        emHeightAscent: 8,
        emHeightDescent: 2,
        hangingBaseline: 6,
        alphabeticBaseline: 0,
        ideographicBaseline: -2
      }};
    }};
  }} catch (error) {{}}

  // Canvas : le rendu GPU est unique (0,00 %). On bruit le readback, pas le
  // dessin, pour que le site voie une empreinte stable mais pas la nôtre.
  try {{
    var read = CanvasRenderingContext2D.prototype.getImageData;
    CanvasRenderingContext2D.prototype.getImageData = function (x, y, w, h) {{
      var data = read.call(this, x, y, w, h);
      var pixels = data.data;
      for (var i = 0; i < pixels.length; i += 4) {{
        pixels[i] = pixels[i] ^ 1;
      }}
      return data;
    }};
    var toData = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function () {{
      var ctx = this.getContext("2d");
      if (ctx) {{
        var img = ctx.getImageData(0, 0, this.width, this.height);
        ctx.putImageData(img, 0, 0);
      }}
      return toData.apply(this, arguments);
    }};
    var toBlob = HTMLCanvasElement.prototype.toBlob;
    HTMLCanvasElement.prototype.toBlob = function (cb) {{
      var ctx = this.getContext("2d");
      if (ctx) {{
        var img = ctx.getImageData(0, 0, this.width, this.height);
        ctx.putImageData(img, 0, 0);
      }}
      return toBlob.apply(this, arguments);
    }};
  }} catch (error) {{}}

  function spoofWebGL(proto) {{
    var getParameter = proto.getParameter;
    proto.getParameter = function (parameter) {{
      var ext = this.getExtension("WEBGL_debug_renderer_info");
      if (ext) {{
        if (parameter === ext.UNMASKED_VENDOR_WEBGL) return "{webgl_vendor}";
        if (parameter === ext.UNMASKED_RENDERER_WEBGL) return "{webgl_renderer}";
      }}
      if (parameter === this.MAX_TEXTURE_SIZE) return {webgl_max_texture_size};
      return getParameter.call(this, parameter);
    }};
    var getExtension = proto.getExtension;
    proto.getExtension = function (name) {{
      if (name === "WEBGL_debug_renderer_info") {{
        return getExtension.call(this, name) || {{
          UNMASKED_VENDOR_WEBGL: 0x9245,
          UNMASKED_RENDERER_WEBGL: 0x9246
        }};
      }}
      return getExtension.call(this, name);
    }};
  }}
  try {{ spoofWebGL(WebGLRenderingContext.prototype); }} catch (error) {{}}
  try {{ spoofWebGL(WebGL2RenderingContext.prototype); }} catch (error) {{}}
}})();"#,
        world = FINGERPRINT_WORLD,
        user_agent = s.user_agent,
        app_version = s.user_agent.trim_start_matches("Mozilla/"),
        platform = s.platform,
        hardware_concurrency = s.hardware_concurrency,
        device_memory = s.device_memory,
        max_touch_points = s.max_touch_points,
        dnt = s.dnt,
        languages = languages,
        screen_width = s.screen_width,
        screen_height = s.screen_height,
        screen_avail_width = s.screen_avail_width,
        screen_avail_height = s.screen_avail_height,
        color_depth = s.color_depth,
        pixel_depth = s.pixel_depth,
        device_pixel_ratio = s.device_pixel_ratio,
        timezone_offset = s.timezone_offset_minutes,
        time_zone = s.time_zone,
        webgl_vendor = s.webgl_vendor,
        webgl_renderer = s.webgl_renderer,
        webgl_max_texture_size = s.webgl_max_texture_size,
    )
}

/// Injection au début du document, tous les frames. Idempotent.
pub fn inject_fingerprint(content: &UserContentManager) {
    content.remove_all_scripts();
    let script = UserScript::new(
        &fingerprint_script(),
        UserContentInjectedFrames::AllFrames,
        UserScriptInjectionTime::Start,
        &[],
        &[],
    );
    content.add_script(&script);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::PRIVACY_FINGERPRINT;

    #[test]
    fn script_pins_every_frozen_value_from_the_const() {
        let js = fingerprint_script();
        for value in [
            PRIVACY_FINGERPRINT.user_agent,
            PRIVACY_FINGERPRINT.platform,
            PRIVACY_FINGERPRINT.time_zone,
            PRIVACY_FINGERPRINT.webgl_vendor,
            PRIVACY_FINGERPRINT.webgl_renderer,
            "1920",
            "1080",
            "1050",
            "24",
            "-120",
        ] {
            assert!(js.contains(value), "le script doit figer {value:?}");
        }
        for language in PRIVACY_FINGERPRINT.languages() {
            assert!(
                js.contains(language),
                "le script doit figer la langue {language}"
            );
        }
    }

    #[test]
    fn script_locks_properties_and_never_randomizes() {
        let js = fingerprint_script();
        assert!(js.contains("Object.defineProperty"));
        assert!(
            js.contains("configurable: false"),
            "les getters doivent être verrouillés"
        );
        assert!(js.contains("navigator.webdriver"));
        for banned in [
            "Math.random",
            "crypto.getRandomValues",
            "Date.now =",
            "performance.now =",
        ] {
            assert!(!js.contains(banned), "aucune randomisation : {banned}");
        }
    }

    #[test]
    fn script_neutralises_the_usual_probes() {
        let js = fingerprint_script();
        assert!(js.contains("hardwareConcurrency"));
        assert!(js.contains("deviceMemory"));
        assert!(js.contains("maxTouchPoints"));
        assert!(js.contains("colorDepth"));
        assert!(js.contains("devicePixelRatio"));
        assert!(js.contains("getTimezoneOffset"));
        assert!(js.contains("getBattery"));
        assert!(js.contains("UNMASKED_VENDOR_WEBGL"));
        assert!(js.contains("UNMASKED_RENDERER_WEBGL"));
        assert!(js.contains("MAX_TEXTURE_SIZE"));
        assert!(js.contains("WebGL2RenderingContext"));
        assert!(js.contains("measureText"));
        assert!(js.contains("getImageData"));
        assert!(js.contains("toDataURL"));
        assert!(js.contains("Permissions.prototype.query"));
        assert!(js.contains("plugins"));
        assert!(js.contains("mimeTypes"));
    }

    #[test]
    fn script_is_compact_and_side_effect_free() {
        let js = fingerprint_script();
        assert!(
            js.len() < 12_000,
            "le script tourne sur chaque page : {}",
            js.len()
        );
        assert!(
            js.contains("try {"),
            "une sonde cassée ne doit pas casser la page"
        );
        assert!(!js.contains("alert("));
        assert!(!js.contains("document.write"));
    }
}
