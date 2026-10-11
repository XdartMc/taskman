//! Accounts in Settings: the new and edit forms, testing the connection before saving, and
//! deleting one no project uses. A project that needs an account it doesn't have waits for this
//! form (`AfterAccount`).

use super::forms::Check;
use super::sync::Msg;
use super::{App, Field, Form, FormKind, SettingsItem};
use crate::store::{self, Account, Config, Project, Provider};
use crate::tracker;

/// What to do with a project once the account it needs is saved.
pub enum AfterAccount {
    Add(Box<Project>),
    Update(usize, Box<Project>),
}

/// Hints and the token default for the account form. A second Linear workspace defaults to its
/// own variable, so two workspaces don't silently share a key.
pub(super) fn account_defaults(form: &mut Form, config: &Config) {
    let linear = form.value("Provider") == "Linear";
    let token = form.value("Token");
    let is_default = token.is_empty()
        || token == "env:JIRA_API_TOKEN"
        || token.starts_with("env:LINEAR_API_KEY");
    if is_default {
        let default = if !linear {
            "env:JIRA_API_TOKEN".to_string()
        } else if config
            .accounts
            .iter()
            .any(|a| a.provider == Provider::Linear && a.token == "env:LINEAR_API_KEY")
        {
            let ws: String = form
                .value("Site")
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
                .collect();
            if ws.is_empty() {
                "env:LINEAR_API_KEY_2".into()
            } else {
                format!("env:LINEAR_API_KEY_{ws}")
            }
        } else {
            "env:LINEAR_API_KEY".to_string()
        };
        form.set("Token", default);
    }
    account_hints(form);
}

/// What to type in the Token field when it holds a token. Never repeats the value.
fn raw_token_hint(provider: Provider) -> String {
    let var = if provider == Provider::Linear { "LINEAR_API_KEY" } else { "JIRA_API_TOKEN" };
    format!(
        "Enter where to read the token from, like env:{var} or cmd:op read …, not the token itself"
    )
}

/// Field hints for the account forms, by provider.
fn account_hints(form: &mut Form) {
    let linear = form.value("Provider") == "Linear";
    let hint = |form: &mut Form, label: &str, text: &str| {
        if let Some(f) = form.fields.iter_mut().find(|f| f.label == label) {
            f.hint = text.into();
        }
    };
    hint(form, "Token", "env:VAR, or cmd:COMMAND that prints it");
    if linear {
        hint(form, "Site", "workspace: the part after linear.app/");
        hint(form, "Email", "not needed for Linear");
    } else {
        hint(form, "Site", "optional: a site to test the connection");
        hint(form, "Email", "your Atlassian email");
    }
}

impl App {
    /// Names of the projects that use account `i`.
    pub(super) fn account_users(&self, i: usize) -> Vec<String> {
        self.projects
            .iter()
            .filter(|p| p.tracker.as_ref().and_then(|t| self.config.account_for(t)) == Some(i))
            .map(|p| p.name.clone())
            .collect()
    }

    pub(super) fn delete_account(&mut self, i: usize) {
        let gone = self.config.accounts.remove(i);
        self.save_config();
        self.rebuild_clients();
        self.toast(format!("Deleted account {}", gone.label()));
    }

    pub(super) fn new_account_form(
        &self,
        provider: Provider,
        site: String,
        then: Option<AfterAccount>,
    ) -> Box<Form> {
        let providers = vec!["Jira".to_string(), "Linear".to_string()];
        let email = self
            .config
            .accounts
            .iter()
            .find(|a| a.provider == Provider::Jira && !a.email.is_empty())
            .map(|a| a.email.clone())
            .or_else(|| std::env::var("JIRA_EMAIL").ok())
            .unwrap_or_default();
        let mut form = Form::new(
            FormKind::Account { then },
            "New account",
            vec![
                Field::choice("Provider", providers, provider.label(), "←/→ to change"),
                Field::text("Site", site, ""),
                Field::text("Email", email, ""),
                Field::text("Token", "", ""),
            ],
        );
        account_defaults(&mut form, &self.config);
        form.focus_on(if form.value("Site").is_empty() {
            "Site"
        } else if provider == Provider::Jira {
            "Email"
        } else {
            "Token"
        });
        Box::new(form)
    }

    pub(super) fn account_settings_form(&self, i: usize) -> Box<Form> {
        let a = &self.config.accounts[i];
        let site = a.provider.site_for_input(&a.site);
        let mut fields = vec![
            Field::choice(
                "Provider",
                vec![a.provider.label().to_string()],
                a.provider.label(),
                "fixed; add a new account to switch",
            ),
            Field::text("Site", site, ""),
        ];
        if a.provider == Provider::Jira {
            fields.push(Field::text("Email", a.email.clone(), ""));
        }
        fields.push(Field::text("Token", a.token.clone(), ""));
        let mut form = Form::new(FormKind::AccountSettings(i), "Account", fields);
        account_hints(&mut form);
        form.focus_on("Site");
        let users = self.account_users(i);
        form.note = Some(if users.is_empty() {
            "Not used by any project".into()
        } else {
            format!("Used by {}", users.join(", "))
        });
        // Saved as typed by an older version: say so where it can be fixed.
        if !store::is_token_source(&a.token) {
            form.error =
                Some(format!("{}, so config.json no longer keeps it", raw_token_hint(a.provider)));
        }
        Box::new(form)
    }

    /// Adds an account (`existing` None) or saves the account at index `existing`.
    pub(super) fn submit_account(
        &mut self,
        form: &mut Form,
        existing: Option<usize>,
    ) -> Result<(), String> {
        let provider = match existing {
            Some(i) => self.config.accounts[i].provider,
            None if form.value("Provider") == "Linear" => Provider::Linear,
            None => Provider::Jira,
        };
        let site = form.value("Site");
        if provider == Provider::Linear && site.is_empty() {
            return Err("Enter the Linear workspace (the part after linear.app/)".into());
        }
        let email = if provider == Provider::Jira { form.value("Email") } else { String::new() };
        if provider == Provider::Jira && email.is_empty() {
            return Err("Enter the email you sign in to Jira with".into());
        }
        // `$NAME` becomes `env:NAME` in the field too, so the form shows what's saved.
        let token = store::env_source_from_shell(&form.value("Token"));
        if token != form.value("Token") {
            form.set("Token", token.clone());
        }
        // Before any test or save: what's saved is where to read the token, never the token, and
        // "save anyway" is for a connection that failed, not for this.
        if !store::is_token_source(&token) {
            return Err(raw_token_hint(provider));
        }
        let site = provider.site_from_input(site);
        let mut account = Account { id: String::new(), provider, site, email, token };

        // Test Jira credentials before saving, against the account's site or the waiting
        // project's; a second Enter saves anyway.
        let test_site = match (&account.site, &form.kind) {
            (s, _) if !s.is_empty() => Some(s.clone()),
            (
                _,
                FormKind::Account { then: Some(AfterAccount::Add(p) | AfterAccount::Update(_, p)) },
            ) => p.tracker.as_ref().map(|t| t.site.clone()),
            _ => None,
        };
        // The test reads the token too, which the clients then use rather than reading it again.
        let verdict = match (provider, test_site.clone()) {
            (Provider::Linear, _) | (Provider::Jira, Some(_)) => match form.checked.clone() {
                Some(Check::Account(v)) => Some(v),
                _ => {
                    let (a, site) = (account.clone(), test_site.clone().unwrap_or_default());
                    self.start_check(form, move |tx| {
                        let token = a.resolve_token();
                        let _ = tx.send(Msg::Token(a.token.clone(), token.clone()));
                        Check::Account(token.and_then(|tok| tracker::whoami(&a, &site, &tok)))
                    });
                    return Ok(());
                }
            },
            _ => None,
        };
        let toast = match (provider, test_site, verdict) {
            (Provider::Linear, _, Some(verdict)) => {
                // The key must belong to the workspace entered.
                let problem = match &verdict {
                    Ok((_, ws)) if !ws.is_empty() && !ws.eq_ignore_ascii_case(&account.site) => {
                        Some(format!(
                            "This key belongs to the workspace {ws}, not {}",
                            account.site
                        ))
                    }
                    Ok(_) => None,
                    Err(e) => Some(e.clone()),
                };
                match (problem, verdict) {
                    (None, Ok((name, _))) => {
                        format!("✓ Connected to Linear {} as {name}", account.site)
                    }
                    (Some(p), _) if !form.save_anyway => {
                        form.save_anyway = true;
                        return Err(format!("{p}. Fix it, or press Enter again to save anyway."));
                    }
                    _ => format!(
                        "Saved the account without a working connection; check {}",
                        store::config_path().display()
                    ),
                }
            }
            (Provider::Jira, Some(site), Some(verdict)) => match verdict {
                Ok((name, _)) => format!("✓ Connected to {} as {name}", store::site_host(&site)),
                Err(e) if !form.save_anyway => {
                    form.save_anyway = true;
                    return Err(format!("{e}. Fix it, or press Enter again to save anyway."));
                }
                Err(_) => {
                    format!(
                        "Saved the account without a working connection; check {}",
                        store::config_path().display()
                    )
                }
            },
            (Provider::Jira, _, _) => {
                // Untested, so read its token again: it may be a new one behind the same source.
                self.trackers.forget(&account.token);
                "Saved the Jira account (add a site to test it)".to_string()
            }
            // The account form doesn't offer Local files, which needs no account; Linear always
            // has a verdict.
            _ => "Saved".to_string(),
        };

        // Edit in place, or reuse an identical account rather than adding a duplicate.
        let same = |a: &Account| {
            a.provider == account.provider
                && a.email == account.email
                && a.token == account.token
                && store::same_site(&a.site, &account.site)
        };
        let index = match existing {
            Some(i) => {
                account.id = self.config.accounts[i].id.clone();
                self.config.accounts[i] = account.clone();
                self.save_config();
                i
            }
            None => match self.config.accounts.iter().position(same) {
                Some(i) => {
                    account.id = self.config.accounts[i].id.clone();
                    i
                }
                None => {
                    account.id = self.config.next_id(provider);
                    self.config.accounts.push(account.clone());
                    self.save_config();
                    self.config.accounts.len() - 1
                }
            },
        };
        self.rebuild_clients();
        for tv in &mut self.tickets {
            tv.jira.request(false);
        }
        self.toast(toast);
        let after = match &mut form.kind {
            FormKind::Account { then } => then.take(),
            _ => None,
        };
        let Some(after) = after else {
            self.settings_show(
                SettingsItem::Account(index),
                self.account_settings_form(index),
                false,
            );
            return Ok(());
        };
        {
            match after {
                AfterAccount::Add(mut project) => {
                    if let Some(t) = &mut project.tracker {
                        t.account = Some(account.id.clone());
                    }
                    self.add_project(*project);
                }
                AfterAccount::Update(p, mut project) => {
                    if let Some(t) = &mut project.tracker {
                        t.account = Some(account.id.clone());
                    }
                    self.update_project(p, *project);
                }
            }
        }
        Ok(())
    }
}
