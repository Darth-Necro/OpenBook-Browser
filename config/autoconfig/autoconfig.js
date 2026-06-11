// SPDX-License-Identifier: MPL-2.0
pref("general.config.filename", "openbook.cfg");
pref("general.config.obscure_value", 0);
// The AutoConfig sandbox stays ENABLED (default): openbook.cfg uses only
// defaultPref()/lockPref(), which the sandbox provides. Running the .cfg
// unsandboxed would be unnecessary privilege (least-privilege, §11).
