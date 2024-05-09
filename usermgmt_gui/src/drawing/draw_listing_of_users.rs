use crate::prelude::*;

use crate::current_selected_view::ConnectionState;
use drawing::draw_utils::GroupDrawing;
use egui_extras::{Size, StripBuilder};
use usermgmt_lib::{
    ldap::{list_ldap_users, LDAPConfig, LdapSearchResult, LdapSimpleCredential},
    slurm::{self, ListedUser},
    ssh::{SshConnection, SshGivenCredential},
};

use crate::{current_selected_view::ListingState, io_resource_manager::IoTaskStatus};

pub fn draw(window: &mut UsermgmtWindow, ui: &mut egui::Ui) {
    draw_readonly_ldap_cred(window, ui);
    ui.separator();
    draw_utils::draw_ssh_credentials(ui, &window.settings, &mut window.ssh_state);
    ldap_list_btn(window, ui);
    slurm_list_btn(window, ui);
    ui.separator();
    let listing_state = &window.listin_state;

    let settings = &window.settings;
    StripBuilder::new(ui)
        .size(
            Size::initial(settings.max_height_listing_table)
                .at_most(settings.max_height_listing_table),
        ) // top cell
        .size(
            Size::initial(settings.max_height_listing_table)
                .at_most(settings.max_height_listing_table),
        ) // top cell
        .vertical(|mut strip| {
            strip.cell(|ui| {
                draw_listed_ldap_users(ui, listing_state, &window.settings);
            });
            strip.cell(|ui| {
                draw_listed_slurm_users(ui, &window.settings, listing_state);
            });
        });

    fn draw_listed_slurm_users(
        ui: &mut egui::Ui,
        setting: &Settings,
        listing_state: &ListingState,
    ) {
        ui.separator();
        let texts = setting.texts();
        draw_utils::draw_status_msg_w_label(
            ui,
            setting,
            texts.status_list_slurm(),
            listing_state.list_slurm_user_res.status(),
            (
                || texts.listing_slurm_init().to_string(),
                || texts.listing_slurm_loading().to_string(),
                |_: &ListedUser| texts.listing_slurm_success().to_string(),
                || texts.listing_slurm_failure().to_string(),
            ),
        );

        listing_state
            .list_slurm_user_res
            .success()
            .inspect(|slurm_users| {
                ui.separator();
                draw_slurm_table(ui, setting, slurm_users)
            });
    }

    fn draw_listed_ldap_users(
        ui: &mut egui::Ui,
        listing_state: &ListingState,
        settings: &Settings,
    ) {
        let texts = settings.texts();
        let status = listing_state.list_ldap_res.status();
        draw_utils::draw_status_msg_w_label(
            ui,
            settings,
            texts.status_list_ldap(),
            status,
            (
                || texts.listing_ldap_init().to_string(),
                || texts.listing_ldap_loading().to_string(),
                |_: &LdapSearchResult| texts.listing_ldap_success().to_string(),
                || texts.listing_ldap_failure().to_string(),
            ),
        );
        if let IoTaskStatus::Successful(ldap_users) = status {
            ui.separator();
            draw_ldap_tables(ui, ldap_users, settings)
        }
    }

    fn draw_slurm_table(ui: &mut egui::Ui, settings: &Settings, slurm_users: &ListedUser) {
        use egui_extras::{Column, TableBuilder};
        draw_table(ui, settings, slurm_users);

        fn draw_table(ui: &mut egui::Ui, settings: &Settings, raw: &ListedUser) {
            let mut table = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.);

            let headers = raw.headers();
            let rows = raw.fields();
            table = table
                .columns(Column::auto(), headers.len().saturating_sub(1))
                .column(Column::remainder());
            table
                .header(settings.header_table_height, |mut header| {
                    for next_title in headers {
                        header.col(|ui| {
                            ui.strong(next_title);
                        });
                    }
                })
                .body(|mut body| {
                    for single_row in rows {
                        body.row(10., |mut row| {
                            for column in single_row {
                                row.col(|ui| _ = ui.label(column));
                            }
                        });
                    }
                });
        }
    }

    fn slurm_list_btn(window: &mut UsermgmtWindow, ui: &mut egui::Ui) {
        let slurm_list_btn_enabled = {
            let listing_state = &window.listin_state;
            let conf_state = &window.conf_state;
            conf_state
                .io_conf
                .success()
                .map(|config| {
                    let which_sys = &window.which_sys;
                    which_sys.is_ssh_cred_provided(window, &config.config, false)
                        && !listing_state.list_slurm_user_res.is_loading()
                })
                .unwrap_or_default()
        };

        let text = window.settings.texts();
        if ui
            .add_enabled(
                slurm_list_btn_enabled,
                egui::Button::new(text.btn_list_slurm_users()),
            )
            .clicked()
        {
            let mgmt_conf = &window
                .conf_state
                .io_conf
                .success()
                .expect("Configuration is loaded before listing slurm user is possible");

            let (username, password) = window
                .ssh_state
                .username_maybe_password()
                .unwrap_or_default();
            let ssh_credentials = SshGivenCredential::new(
                username,
                password.unwrap_or_default(),
                usermgmt_lib::ssh::create_ssh_key_pair_conf(
                    window.ssh_state.ssh_key_pair(),
                    &mgmt_conf.config,
                ),
            );
            let mgmt_conf = mgmt_conf.config.clone();
            let failed_parsing_slurm = text.failed_parsing_slurm().clone();
            _ = window.listin_state.list_slurm_user_res.spawn_task(
                move || {
                    let connection = SshConnection::from_head_node(&mgmt_conf, ssh_credentials);
                    let slurm_users_raw = slurm::list_users(&mgmt_conf, &connection, true)?;
                    ListedUser::new(&slurm_users_raw).ok_or(anyhow!(failed_parsing_slurm))
                },
                String::from("Getting slurm user"),
            );
            let settings = &window.settings;
            draw_utils::tooltip_widget(ui, settings, settings.tooltiptexts().list_ssh_btn());
        }
    }

    fn ldap_list_btn(window: &mut UsermgmtWindow, ui: &mut egui::Ui) {
        let list_ldap_btn_enabled = {
            let list_state = &window.listin_state;
            let no_ldpa_loading = !list_state.list_ldap_res.is_loading();
            let configuration_is_loaded = window.conf_state.io_conf.is_there();
            list_state.rw_user_name.is_some()
                && list_state.rw_pw.is_some()
                && no_ldpa_loading
                && configuration_is_loaded
        };

        let text = window.settings.texts();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    list_ldap_btn_enabled,
                    egui::Button::new(text.btn_list_ldap_users()),
                )
                .clicked()
            {
                let mgmt_conf = &window
                    .conf_state
                    .io_conf
                    .success()
                    .expect("Configuration is loaded before this action is triggered");

                let listing_state = &window.listin_state;
                let (username, password) = (
                    listing_state.rw_user_name.clone().unwrap_or_default(),
                    listing_state.rw_pw.clone().unwrap_or_default(),
                );
                let mgmt_conf = mgmt_conf.config.clone();
                window.listin_state.list_ldap_res.spawn_task(
                    move || {
                        let config = LDAPConfig::new(
                            &mgmt_conf,
                            LdapSimpleCredential::new(username, password),
                        )?;
                        list_ldap_users(&config)
                    },
                    "Listing ldap user".to_owned(),
                );
            };
            let settings = &window.settings;
            draw_utils::tooltip_widget(ui, settings, settings.tooltiptexts().list_ldap_btn());
        });
    }

    fn draw_readonly_ldap_cred(window: &mut UsermgmtWindow, ui: &mut egui::Ui) {
        let (conf_user_name, conf_pw) = window
            .conf_state
            .io_conf
            .success()
            .map(|configuration| {
                let configuration = &configuration.config;
                (
                    configuration.ldap_readonly_user.as_deref(),
                    configuration.ldap_readonly_pw.as_deref(),
                )
            })
            .unwrap_or_default();
        let mut rw_user =
            field_conf_or_state(window.listin_state.rw_user_name.as_deref(), conf_user_name);
        let mut rw_password = field_conf_or_state(window.listin_state.rw_pw.as_deref(), conf_pw);
        let settings = &window.settings;
        draw_utils::user_password_box(
            ui,
            settings,
            &GroupDrawing::new(settings.texts().readonly_ldap_cred())
                .add_tooltip(settings.tooltiptexts().ldap_readonly_creds()),
            &mut rw_user,
            &mut rw_password,
        );
        window.listin_state.rw_user_name = rw_user;
        window.listin_state.rw_pw = rw_password;
    }

    fn field_conf_or_state(from_window: Option<&str>, from_conf: Option<&str>) -> Option<String> {
        let no_owned = match from_window {
            None => from_conf,
            some => some,
        };
        no_owned.map(|unowned| unowned.to_string())
    }

    fn draw_ldap_tables(ui: &mut egui::Ui, raw: &LdapSearchResult, settings: &Settings) {
        use egui_extras::{Column, TableBuilder};
        draw_table(ui, raw, settings);

        fn draw_table(ui: &mut egui::Ui, raw: &LdapSearchResult, settings: &Settings) {
            // Need to give manual id otherwise the next table causes a clash
            // on the scroll aread id.
            // Reference: https://docs.rs/egui_extras/latest/egui_extras/struct.TableBuilder.html
            ui.push_id(1, |ui| {
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .min_scrolled_height(0.);

                let headers = raw.headers();
                let rows = raw.fields();
                table = table
                    .columns(Column::auto(), headers.len().saturating_sub(1))
                    .column(Column::remainder());
                table
                    .header(settings.header_table_height, |mut header| {
                        for &next_title in headers.iter() {
                            header.col(|ui| {
                                ui.strong(next_title);
                            });
                        }
                    })
                    .body(|mut body| {
                        for single_row in rows.iter() {
                            body.row(10., |mut row| {
                                for column in single_row {
                                    row.col(|ui| {
                                        _ = ui.label(column.join(&settings.ldap_multi_field_sep))
                                    });
                                }
                            });
                        }
                    });
            });
        }
    }
}
