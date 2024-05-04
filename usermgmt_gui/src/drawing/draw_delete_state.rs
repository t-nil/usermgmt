use usermgmt_lib::operations;

use crate::prelude::*;

use super::draw_utils::{GroupDrawing, TextFieldEntry};

pub fn draw(ui: &mut egui::Ui, window: &mut UsermgmtWindow) {
    let allow_deletion = {
        let remove_state = &mut window.remove_state;
        let settings = &window.settings;
        let tooltips = settings.tooltiptexts();
        draw_utils::draw_box_group(ui, settings, &GroupDrawing::new("Required"), |ui| {
            draw_utils::entry_field(
                ui,
                &window.settings,
                &mut TextFieldEntry::new("Username", &mut remove_state.username)
                    .with_tooltip(tooltips.username()),
            );
        });
        !remove_state.username.trim().is_empty()
    };
    draw_utils::draw_credentials(ui, window, false);
    ui.add_enabled_ui(allow_deletion, |ui| {
        let text = window.settings.texts();
        if ui.button(text.btn_action_remove()).clicked() {
            delete_user(window)
        }
    });
    let remove_state = &mut window.remove_state;
    let last_username = &remove_state.last_username;
    draw_utils::draw_status_msg(
        ui,
        &window.settings,
        remove_state.remove_res_io.status(),
        (
            || "No user remove yet".to_owned(),
            || format!("In the process of removing user ({}).", last_username),
            |username: &String| format!("Removed user ({}) !", username),
            || format!("Failed to remove user ({}).", last_username),
        ),
    );
}

fn delete_user(window: &mut UsermgmtWindow) {
    window
        .remove_state
        .last_username
        .clone_from(&window.remove_state.username);
    if let Ok(prep) =
        general_utils::prep_conf_creds(window, |app| &mut app.remove_state.remove_res_io, false)
    {
        let username = window.remove_state.username.clone();
        let _ = window.remove_state.remove_res_io.spawn_task(
            move || {
                operations::delete_user(
                    &username,
                    &prep.on_which_sys,
                    &prep.config,
                    prep.ldap_cred,
                    prep.ssh_cred,
                )?;
                Ok(username)
            },
            String::from("Deleting user"),
        );
    }
}
