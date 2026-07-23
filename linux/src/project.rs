use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
pub struct ProjectModel {
    pub display_name: String,
    pub root_path: String,
    pub modules: Vec<ProjectModule>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectModule {
    pub id: String,
    pub display_name: String,
    pub path: String,
    pub files: Vec<ProjectNode>,
    pub targets: Vec<ProjectTarget>,
    pub configurations: Vec<ProjectBuildConfiguration>,
    pub schemes: Vec<ProjectScheme>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectNode {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub kind: String,
    pub exists: bool,
    pub children: Vec<ProjectNode>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectTarget {
    pub id: String,
    pub name: String,
    pub product_type: String,
    pub bundle_identifier: Option<String>,
    pub deployment_target: Option<String>,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectBuildConfiguration {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub target_id: Option<String>,
    pub settings: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectScheme {
    pub name: String,
    pub shared: bool,
    pub target_ids: Vec<String>,
}

#[derive(Clone, Debug)]
enum OpenStepValue {
    String(String),
    Array(Vec<OpenStepValue>),
    Dict(BTreeMap<String, OpenStepValue>),
}

impl OpenStepValue {
    fn string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    fn array(&self) -> Option<&[OpenStepValue]> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    fn dict(&self) -> Option<&BTreeMap<String, OpenStepValue>> {
        match self {
            Self::Dict(value) => Some(value),
            _ => None,
        }
    }
}

struct OpenStepParser<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> OpenStepParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            offset: 0,
        }
    }

    fn parse(mut self) -> Result<OpenStepValue, String> {
        self.skip_trivia()?;
        let value = self.parse_value()?;
        self.skip_trivia()?;
        Ok(value)
    }

    fn parse_value(&mut self) -> Result<OpenStepValue, String> {
        self.skip_trivia()?;
        match self.peek() {
            Some(b'{') => self.parse_dict(),
            Some(b'(') => self.parse_array(),
            Some(b'"') => self.parse_quoted().map(OpenStepValue::String),
            Some(_) => self.parse_atom().map(OpenStepValue::String),
            None => Err("unexpected end of OpenStep plist".to_string()),
        }
    }

    fn parse_dict(&mut self) -> Result<OpenStepValue, String> {
        self.expect(b'{')?;
        let mut values = BTreeMap::new();
        loop {
            self.skip_trivia()?;
            if self.consume(b'}') {
                break;
            }
            let key = if self.peek() == Some(b'"') {
                self.parse_quoted()?
            } else {
                self.parse_atom()?
            };
            self.skip_trivia()?;
            self.expect(b'=')?;
            let value = self.parse_value()?;
            self.skip_trivia()?;
            self.expect(b';')?;
            values.insert(key, value);
        }
        Ok(OpenStepValue::Dict(values))
    }

    fn parse_array(&mut self) -> Result<OpenStepValue, String> {
        self.expect(b'(')?;
        let mut values = Vec::new();
        loop {
            self.skip_trivia()?;
            if self.consume(b')') {
                break;
            }
            values.push(self.parse_value()?);
            self.skip_trivia()?;
            if self.consume(b',') {
                continue;
            }
            if self.peek() != Some(b')') {
                return Err(self.error("expected ',' or ')'"));
            }
        }
        Ok(OpenStepValue::Array(values))
    }

    fn parse_quoted(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut value = Vec::new();
        loop {
            let byte = self
                .next()
                .ok_or_else(|| self.error("unterminated quoted string"))?;
            match byte {
                b'"' => break,
                b'\\' => {
                    let escaped = self
                        .next()
                        .ok_or_else(|| self.error("unterminated escape"))?;
                    value.push(match escaped {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        other => other,
                    });
                }
                other => value.push(other),
            }
        }
        String::from_utf8(value).map_err(|_| self.error("quoted string is not UTF-8"))
    }

    fn parse_atom(&mut self) -> Result<String, String> {
        let start = self.offset;
        while let Some(byte) = self.peek() {
            if byte.is_ascii_whitespace()
                || matches!(byte, b'{' | b'}' | b'(' | b')' | b'=' | b';' | b',')
            {
                break;
            }
            if byte == b'/' && matches!(self.bytes.get(self.offset + 1), Some(b'/') | Some(b'*')) {
                break;
            }
            self.offset += 1;
        }
        if self.offset == start {
            return Err(self.error("expected atom"));
        }
        String::from_utf8(self.bytes[start..self.offset].to_vec())
            .map_err(|_| self.error("atom is not UTF-8"))
    }

    fn skip_trivia(&mut self) -> Result<(), String> {
        loop {
            while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                self.offset += 1;
            }
            if self.peek() != Some(b'/') {
                return Ok(());
            }
            match self.bytes.get(self.offset + 1).copied() {
                Some(b'/') => {
                    self.offset += 2;
                    while let Some(byte) = self.next() {
                        if byte == b'\n' {
                            break;
                        }
                    }
                }
                Some(b'*') => {
                    self.offset += 2;
                    let mut closed = false;
                    while self.offset + 1 < self.bytes.len() {
                        if self.bytes[self.offset] == b'*' && self.bytes[self.offset + 1] == b'/' {
                            self.offset += 2;
                            closed = true;
                            break;
                        }
                        self.offset += 1;
                    }
                    if !closed {
                        return Err(self.error("unterminated comment"));
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        self.skip_trivia()?;
        if self.consume(expected) {
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}'", expected as char)))
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let value = self.peek()?;
        self.offset += 1;
        Some(value)
    }

    fn error(&self, message: &str) -> String {
        format!("{message} at byte {}", self.offset)
    }
}

pub fn load_project(path: &Path) -> Result<ProjectModel, String> {
    let artifact = resolve_project_artifact(path)?;
    let extension = artifact
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let project_paths = if extension == "xcworkspace" {
        workspace_project_paths(&artifact)?
    } else if extension == "xcodeproj" {
        vec![artifact.clone()]
    } else {
        return Err(format!("Unsupported project at {}", artifact.display()));
    };
    let modules = project_paths
        .iter()
        .map(|project| load_xcode_project(project))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProjectModel {
        display_name: artifact
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Project")
            .to_string(),
        root_path: artifact.display().to_string(),
        modules,
    })
}

fn resolve_project_artifact(path: &Path) -> Result<PathBuf, String> {
    if matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("xcodeproj" | "xcworkspace")
    ) {
        return Ok(path.to_path_buf());
    }
    let entries = fs::read_dir(path).map_err(|err| {
        format!(
            "Failed to inspect project directory {}: {err}",
            path.display()
        )
    })?;
    let mut artifacts = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            matches!(
                candidate.extension().and_then(|value| value.to_str()),
                Some("xcworkspace" | "xcodeproj")
            )
        })
        .collect::<Vec<_>>();
    artifacts.sort_by_key(|candidate| {
        (
            candidate.extension().and_then(|value| value.to_str()) != Some("xcworkspace"),
            candidate.clone(),
        )
    });
    artifacts
        .into_iter()
        .next()
        .ok_or_else(|| format!("No Xcode project found in {}", path.display()))
}

fn workspace_project_paths(workspace: &Path) -> Result<Vec<PathBuf>, String> {
    let data_path = workspace.join("contents.xcworkspacedata");
    let data = fs::read_to_string(&data_path)
        .map_err(|err| format!("Failed to read {}: {err}", data_path.display()))?;
    let base = workspace.parent().unwrap_or_else(|| Path::new("."));
    let mut projects = Vec::new();
    for fragment in data.split("location").skip(1) {
        let Some(value) = xml_attribute_value(fragment, "") else {
            continue;
        };
        let location = value
            .strip_prefix("group:")
            .or_else(|| value.strip_prefix("container:"))
            .unwrap_or(&value);
        if location.ends_with(".xcodeproj") {
            let candidate = base.join(location);
            if candidate.exists() && !projects.contains(&candidate) {
                projects.push(candidate);
            }
        }
    }
    if projects.is_empty() {
        return Err(format!(
            "Workspace {} contains no projects",
            workspace.display()
        ));
    }
    Ok(projects)
}

fn load_xcode_project(project: &Path) -> Result<ProjectModule, String> {
    let pbx_path = project.join("project.pbxproj");
    let contents = fs::read_to_string(&pbx_path)
        .map_err(|err| format!("Failed to read {}: {err}", pbx_path.display()))?;
    let root = OpenStepParser::new(&contents).parse()?;
    let root = root
        .dict()
        .ok_or_else(|| "project root is not a dictionary".to_string())?;
    let objects = root
        .get("objects")
        .and_then(OpenStepValue::dict)
        .ok_or_else(|| "project has no objects dictionary".to_string())?;
    let project_id = root
        .get("rootObject")
        .and_then(OpenStepValue::string)
        .ok_or_else(|| "project has no rootObject".to_string())?;
    let project_object = object(objects, project_id)?;
    let source_root = project.parent().unwrap_or_else(|| Path::new("."));
    let main_group_id = value_string(project_object, "mainGroup").unwrap_or_default();
    let files = if main_group_id.is_empty() {
        Vec::new()
    } else {
        vec![build_node(
            objects,
            main_group_id,
            source_root,
            source_root,
            &mut HashSet::new(),
        )?]
    };

    let mut configurations = Vec::new();
    if let Some(config_list) = value_string(project_object, "buildConfigurationList") {
        configurations.extend(load_configurations(objects, config_list, "project", None));
    }
    let mut targets = Vec::new();
    for target_id in value_strings(project_object, "targets") {
        let Ok(target) = object(objects, &target_id) else {
            continue;
        };
        let name = value_string(target, "name")
            .unwrap_or(&target_id)
            .to_string();
        let target_configs = value_string(target, "buildConfigurationList")
            .map(|list| load_configurations(objects, list, "target", Some(&target_id)))
            .unwrap_or_default();
        let bundle_identifier = first_setting(&target_configs, "PRODUCT_BUNDLE_IDENTIFIER");
        let deployment_target = target_configs.iter().find_map(|config| {
            [
                "MACOSX_DEPLOYMENT_TARGET",
                "IPHONEOS_DEPLOYMENT_TARGET",
                "TVOS_DEPLOYMENT_TARGET",
                "WATCHOS_DEPLOYMENT_TARGET",
            ]
            .iter()
            .find_map(|key| config.settings.get(*key).cloned())
        });
        let dependencies = value_strings(target, "dependencies")
            .into_iter()
            .filter_map(|dependency_id| object(objects, &dependency_id).ok())
            .filter_map(|dependency| value_string(dependency, "target").map(ToString::to_string))
            .collect();
        targets.push(ProjectTarget {
            id: target_id.clone(),
            name,
            product_type: map_product_type(value_string(target, "productType").unwrap_or_default()),
            bundle_identifier,
            deployment_target,
            dependencies,
        });
        configurations.extend(target_configs);
    }

    Ok(ProjectModule {
        id: project.display().to_string(),
        display_name: project
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Project")
            .to_string(),
        path: project.display().to_string(),
        files,
        targets,
        configurations,
        schemes: load_schemes(project),
    })
}

fn object<'a>(
    objects: &'a BTreeMap<String, OpenStepValue>,
    id: &str,
) -> Result<&'a BTreeMap<String, OpenStepValue>, String> {
    objects
        .get(id)
        .and_then(OpenStepValue::dict)
        .ok_or_else(|| format!("missing project object {id}"))
}

fn value_string<'a>(dict: &'a BTreeMap<String, OpenStepValue>, key: &str) -> Option<&'a str> {
    dict.get(key).and_then(OpenStepValue::string)
}

fn value_strings(dict: &BTreeMap<String, OpenStepValue>, key: &str) -> Vec<String> {
    dict.get(key)
        .and_then(OpenStepValue::array)
        .into_iter()
        .flatten()
        .filter_map(OpenStepValue::string)
        .map(ToString::to_string)
        .collect()
}

fn build_node(
    objects: &BTreeMap<String, OpenStepValue>,
    id: &str,
    parent_path: &Path,
    source_root: &Path,
    visiting: &mut HashSet<String>,
) -> Result<ProjectNode, String> {
    if !visiting.insert(id.to_string()) {
        return Err(format!("project group cycle at {id}"));
    }
    let item = object(objects, id)?;
    let isa = value_string(item, "isa").unwrap_or_default();
    let raw_path = value_string(item, "path");
    let name = value_string(item, "name")
        .or(raw_path)
        .unwrap_or(id)
        .trim_matches('"')
        .to_string();
    let source_tree = value_string(item, "sourceTree").unwrap_or("<group>");
    let resolved = match source_tree.trim_matches('"') {
        "<absolute>" => raw_path.map(PathBuf::from),
        "SOURCE_ROOT" => raw_path.map(|path| source_root.join(path)),
        "<group>" => raw_path
            .map(|path| parent_path.join(path))
            .or_else(|| Some(parent_path.to_path_buf())),
        _ => None,
    };
    let is_group = isa.contains("Group");
    let child_parent = resolved.as_deref().unwrap_or(parent_path);
    let children = if is_group {
        value_strings(item, "children")
            .into_iter()
            .filter_map(|child| {
                build_node(objects, &child, child_parent, source_root, visiting).ok()
            })
            .collect()
    } else {
        Vec::new()
    };
    visiting.remove(id);
    Ok(ProjectNode {
        id: id.to_string(),
        name,
        path: resolved.as_ref().map(|path| path.display().to_string()),
        kind: if is_group { "group" } else { "file" }.to_string(),
        exists: resolved.as_deref().is_some_and(Path::exists),
        children,
    })
}

fn load_configurations(
    objects: &BTreeMap<String, OpenStepValue>,
    list_id: &str,
    scope: &str,
    target_id: Option<&str>,
) -> Vec<ProjectBuildConfiguration> {
    let Ok(list) = object(objects, list_id) else {
        return Vec::new();
    };
    value_strings(list, "buildConfigurations")
        .into_iter()
        .filter_map(|config_id| {
            let config = object(objects, &config_id).ok()?;
            let settings = config
                .get("buildSettings")
                .and_then(OpenStepValue::dict)
                .map(|settings| {
                    settings
                        .iter()
                        .filter_map(|(key, value)| {
                            let rendered = match value {
                                OpenStepValue::String(value) => value.clone(),
                                OpenStepValue::Array(values) => values
                                    .iter()
                                    .filter_map(OpenStepValue::string)
                                    .collect::<Vec<_>>()
                                    .join(" "),
                                OpenStepValue::Dict(_) => return None,
                            };
                            Some((key.clone(), rendered))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(ProjectBuildConfiguration {
                id: config_id,
                name: value_string(config, "name")
                    .unwrap_or("Configuration")
                    .to_string(),
                scope: scope.to_string(),
                target_id: target_id.map(ToString::to_string),
                settings,
            })
        })
        .collect()
}

fn first_setting(configurations: &[ProjectBuildConfiguration], key: &str) -> Option<String> {
    configurations
        .iter()
        .find_map(|config| config.settings.get(key))
        .filter(|value| !value.contains("$("))
        .cloned()
}

fn map_product_type(raw: &str) -> String {
    match raw.trim_matches('"') {
        "com.apple.product-type.application" => "application",
        "com.apple.product-type.framework" => "framework",
        "com.apple.product-type.library.static" => "staticLibrary",
        "com.apple.product-type.library.dynamic" => "dynamicLibrary",
        "com.apple.product-type.bundle.unit-test" => "unitTest",
        "com.apple.product-type.bundle.ui-testing" => "uiTest",
        "com.apple.product-type.tool" => "commandLineTool",
        "com.apple.product-type.app-extension" => "appExtension",
        "com.apple.product-type.bundle" => "bundle",
        _ => "other",
    }
    .to_string()
}

fn load_schemes(project: &Path) -> Vec<ProjectScheme> {
    let mut schemes = Vec::new();
    collect_schemes(&project.join("xcshareddata/xcschemes"), true, &mut schemes);
    let user_data = project.join("xcuserdata");
    if let Ok(users) = fs::read_dir(user_data) {
        for user in users.flatten() {
            collect_schemes(&user.path().join("xcschemes"), false, &mut schemes);
        }
    }
    schemes.sort_by(|left, right| left.name.cmp(&right.name));
    schemes
}

fn collect_schemes(path: &Path, shared: bool, output: &mut Vec<ProjectScheme>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("xcscheme") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let contents = fs::read_to_string(&path).unwrap_or_default();
        let mut target_ids = Vec::new();
        for fragment in contents.split("BlueprintIdentifier").skip(1) {
            if let Some(id) = xml_attribute_value(fragment, "") {
                if !target_ids.contains(&id) {
                    target_ids.push(id);
                }
            }
        }
        output.push(ProjectScheme {
            name: name.to_string(),
            shared,
            target_ids,
        });
    }
}

fn xml_attribute_value(fragment: &str, _name: &str) -> Option<String> {
    let equals = fragment.find('=')?;
    let rest = fragment[equals + 1..].trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value = &rest[quote.len_utf8()..];
    let end = value.find(quote)?;
    Some(value[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openstep_parser_handles_comments_arrays_and_quoted_settings() {
        let parsed = OpenStepParser::new(
            r#"// !$*UTF8*$!
            { rootObject = A /* Project */; objects = {
                A = { isa = PBXProject; targets = ( B /* App */, ); };
                B = { isa = PBXNativeTarget; name = "Demo App"; buildSettings = { KEY = "a b"; }; };
            }; }"#,
        )
        .parse()
        .expect("parse fixture");
        let root = parsed.dict().unwrap();
        assert_eq!(
            root.get("rootObject").and_then(OpenStepValue::string),
            Some("A")
        );
        assert_eq!(
            root.get("objects")
                .and_then(OpenStepValue::dict)
                .and_then(|objects| objects.get("B"))
                .and_then(OpenStepValue::dict)
                .and_then(|target| target.get("name"))
                .and_then(OpenStepValue::string),
            Some("Demo App")
        );
    }

    #[test]
    fn loads_repository_xcode_project_when_available() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cmux.xcodeproj");
        if !project.exists() {
            return;
        }
        let model = load_project(&project).expect("load cmux project");
        assert_eq!(model.modules.len(), 1);
        assert!(!model.modules[0].targets.is_empty());
        assert!(model.modules[0]
            .targets
            .iter()
            .any(|target| target.name == "cmux"));
        assert!(!model.modules[0].files.is_empty());
    }
}
