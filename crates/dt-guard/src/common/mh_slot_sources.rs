//! Where the MH slot model's script-owned names live — ONE home for the paths
//! and constant names the MH guards read (`kustomize_mh_template`,
//! `mh_slot_model`). POLICY-FREE: values are read from the scripts through
//! [`crate::common::shell_consts::shell_const`], never retyped here.

/// The Kind DELIVERY script (template split, apply set, slot table).
pub const DEPLOY_SH: &str = "infra/kind/scripts/deploy.sh";
/// The MH pool tool (instantiates the template per slot).
pub const MH_SH: &str = "infra/kind/scripts/mh.sh";
/// [`DEPLOY_SH`] constant: the template Deployment's name.
pub const TEMPLATE_NAME_CONST: &str = "MH_TEMPLATE_DEPLOYMENT";
/// [`DEPLOY_SH`] constant: the label marking template INPUTS (never applied).
pub const TEMPLATE_INPUT_LABEL_CONST: &str = "MH_TEMPLATE_INPUT_LABEL";
/// [`DEPLOY_SH`] constant: the label carrying a slot ConfigMap's slot id.
pub const SLOT_LABEL_CONST: &str = "MH_SLOT_LABEL";
/// [`MH_SH`] constant: the container port that gets the slot hostPort.
pub const WT_PORT_NAME_CONST: &str = "MH_WT_PORT_NAME";
