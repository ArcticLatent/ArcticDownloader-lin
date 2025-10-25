use std::{
    cell::{Cell, RefCell},
    fs,
    path::PathBuf,
    rc::Rc,
};

use adw::{
    gtk::{self, prelude::*, Align, Orientation, ResponseType, SelectionMode, StackTransitionType},
    prelude::ActionRowExt,
    Application, ApplicationWindow, HeaderBar, Toast, ToastOverlay,
};
use anyhow::{anyhow, Context, Result};
use arctic_downloader::model::{
    LoraDefinition, MasterModel, ModelArtifact, ModelCatalog, ModelVariant, QualityTier,
    TargetCategory,
};

const APP_ID: &str = "dev.wknd.CatalogAdmin";
const DEFAULT_CATALOG_PATH: &str = "data/catalog.json";

fn main() -> Result<()> {
    env_logger::init();
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        if let Err(err) = build_admin_ui(app) {
            eprintln!("Failed to initialize catalog admin: {err:?}");
        }
    });
    app.run();
    Ok(())
}

#[derive(Clone)]
struct CatalogState {
    path: PathBuf,
    catalog: ModelCatalog,
    dirty: bool,
}

type SharedState = Rc<RefCell<CatalogState>>;

#[derive(Clone)]
struct ArtifactRow {
    container: gtk::Box,
    target_entry: gtk::Entry,
    url_entry: gtk::Entry,
    sha256: Option<String>,
    size_bytes: Option<u64>,
    license_url: Option<String>,
    original_repo: Option<String>,
    original_path: Option<String>,
}

#[derive(Clone)]
struct ArtifactRowPreset {
    target_slug: String,
    url: String,
    repo: Option<String>,
    path: Option<String>,
    sha256: Option<String>,
    size_bytes: Option<u64>,
    license_url: Option<String>,
}

fn build_admin_ui(app: &Application) -> Result<()> {
    let path = PathBuf::from(DEFAULT_CATALOG_PATH);
    let catalog = if path.exists() {
        load_catalog(&path)?
    } else {
        ModelCatalog {
            catalog_version: 1,
            models: Vec::new(),
            loras: Vec::new(),
        }
    };

    let state = Rc::new(RefCell::new(CatalogState {
        path,
        catalog,
        dirty: false,
    }));

    let overlay = ToastOverlay::new();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Catalog Admin")
        .default_width(960)
        .default_height(720)
        .content(&overlay)
        .build();

    let header = HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("Catalog Admin"))));

    let add_model_button = gtk::Button::with_label("Add Model");
    header.pack_start(&add_model_button);

    let add_lora_button = gtk::Button::with_label("Add LoRA");
    header.pack_start(&add_lora_button);

    let save_button = gtk::Button::with_label("Save Catalog");
    header.pack_end(&save_button);

    let reload_button = gtk::Button::with_label("Reload");
    header.pack_end(&reload_button);

    let container = gtk::Box::new(Orientation::Vertical, 12);
    container.append(&header);

    let stack_switcher = gtk::StackSwitcher::new();
    stack_switcher.set_halign(Align::Start);
    stack_switcher.set_margin_start(8);
    stack_switcher.set_margin_top(8);
    stack_switcher.set_margin_bottom(4);
    container.append(&stack_switcher);

    let stack = gtk::Stack::builder()
        .transition_type(StackTransitionType::SlideLeftRight)
        .hexpand(true)
        .vexpand(true)
        .build();
    stack_switcher.set_stack(Some(&stack));

    let model_list_box = gtk::ListBox::new();
    model_list_box.set_selection_mode(SelectionMode::None);
    let model_scroller = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&model_list_box)
        .build();
    stack.add_titled(&model_scroller, Some("models"), "Models");

    let lora_list_box = gtk::ListBox::new();
    lora_list_box.set_selection_mode(SelectionMode::None);
    let lora_scroller = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&lora_list_box)
        .build();
    stack.add_titled(&lora_scroller, Some("loras"), "LoRAs");

    container.append(&stack);
    overlay.set_child(Some(&container));

    refresh_model_list(&model_list_box, &state, &overlay, &window);
    refresh_lora_list(&lora_list_box, &state, &overlay);

    {
        let state = state.clone();
        let overlay = overlay.clone();
        let window = window.clone();
        let model_list_box = model_list_box.clone();
        add_model_button.connect_clicked(move |_| {
            if let Err(err) = edit_model_dialog(None, &state, &overlay, &window) {
                overlay.add_toast(Toast::new(&format!("Failed to add model: {err}")));
            }
            refresh_model_list(&model_list_box, &state, &overlay, &window);
        });
    }

    {
        let state = state.clone();
        let overlay = overlay.clone();
        let window = window.clone();
        let model_list_box = model_list_box.clone();
        let lora_list_box = lora_list_box.clone();
        add_lora_button.connect_clicked(move |_| match edit_lora_dialog(None, &state, &overlay) {
            Ok(true) => {
                refresh_lora_list(&lora_list_box, &state, &overlay);
                refresh_model_list(&model_list_box, &state, &overlay, &window);
            }
            Ok(false) => {}
            Err(err) => {
                overlay.add_toast(Toast::new(&format!("Failed to add LoRA: {err}")));
            }
        });
    }

    {
        let state = state.clone();
        let overlay = overlay.clone();
        let window = window.clone();
        let model_list_box = model_list_box.clone();
        let lora_list_box = lora_list_box.clone();
        save_button.connect_clicked(move |_| {
            let result = {
                let state_ref = state.borrow();
                save_catalog(&state_ref)
            };

            match result {
                Ok(()) => {
                    state.borrow_mut().dirty = false;
                    overlay.add_toast(Toast::new("Catalog saved."));
                }
                Err(err) => {
                    overlay.add_toast(Toast::new(&format!("Save failed: {err}")));
                }
            }

            refresh_model_list(&model_list_box, &state, &overlay, &window);
            refresh_lora_list(&lora_list_box, &state, &overlay);
        });
    }

    {
        let state = state.clone();
        let overlay = overlay.clone();
        let window = window.clone();
        let model_list_box = model_list_box.clone();
        let lora_list_box = lora_list_box.clone();
        reload_button.connect_clicked(move |_| {
            let path = state.borrow().path.clone();
            match load_catalog(&path) {
                Ok(new_catalog) => {
                    {
                        let mut state_mut = state.borrow_mut();
                        state_mut.catalog = new_catalog;
                        state_mut.dirty = false;
                    }
                    overlay.add_toast(Toast::new("Catalog reloaded."));
                    refresh_model_list(&model_list_box, &state, &overlay, &window);
                    refresh_lora_list(&lora_list_box, &state, &overlay);
                }
                Err(err) => {
                    overlay.add_toast(Toast::new(&format!("Reload failed: {err}")));
                }
            }
        });
    }

    window.present();
    Ok(())
}

fn load_catalog(path: &PathBuf) -> Result<ModelCatalog> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read catalog at {:?}", path))?;
    let parsed: ModelCatalog =
        serde_json::from_str(&contents).with_context(|| "Catalog JSON is invalid".to_string())?;
    Ok(parsed)
}

fn save_catalog(state: &CatalogState) -> Result<()> {
    let data = serde_json::to_string_pretty(&state.catalog)?;
    fs::write(&state.path, data)
        .with_context(|| format!("Failed to write catalog to {:?}", state.path))?;
    Ok(())
}

fn refresh_model_list(
    list: &gtk::ListBox,
    state: &SharedState,
    overlay: &ToastOverlay,
    parent: &ApplicationWindow,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for (idx, model) in state.borrow().catalog.models.iter().enumerate() {
        let row = adw::ActionRow::builder()
            .title(&model.display_name)
            .subtitle(&format!(
                "{} • {} variant(s)",
                model.id,
                model.variants.len()
            ))
            .activatable(false)
            .build();

        let edit_button = gtk::Button::with_label("Edit");
        edit_button.set_halign(Align::Center);
        row.add_suffix(&edit_button);

        let duplicate_button = gtk::Button::with_label("Duplicate");
        duplicate_button.set_halign(Align::Center);
        row.add_suffix(&duplicate_button);

        let delete_button = gtk::Button::with_label("Remove");
        delete_button.set_halign(Align::Center);
        row.add_suffix(&delete_button);

        {
            let state = state.clone();
            let list = list.clone();
            let overlay = overlay.clone();
            let parent = parent.clone();
            edit_button.connect_clicked(move |_| {
                if let Err(err) = edit_model_dialog(Some(idx), &state, &overlay, &parent) {
                    overlay.add_toast(Toast::new(&format!("Failed to edit model: {err}")));
                }
                refresh_model_list(&list, &state, &overlay, &parent);
            });
        }

        {
            let state = state.clone();
            let list = list.clone();
            let overlay = overlay.clone();
            let parent = parent.clone();
            duplicate_button.connect_clicked(move |_| {
                let duplicated = {
                    let state_ref = state.borrow();
                    state_ref.catalog.models.get(idx).cloned().map(|mut model| {
                        model.id = generate_model_copy_id(&model.id, &state_ref.catalog.models);
                        model.display_name = format!("{} (Copy)", model.display_name.trim_end());
                        model
                    })
                };
                if let Some(model) = duplicated {
                    let mut state_mut = state.borrow_mut();
                    state_mut.catalog.models.push(model);
                    state_mut.dirty = true;
                    overlay.add_toast(Toast::new("Model duplicated."));
                }
                refresh_model_list(&list, &state, &overlay, &parent);
            });
        }

        {
            let state = state.clone();
            let list = list.clone();
            let overlay = overlay.clone();
            let parent = parent.clone();
            delete_button.connect_clicked(move |_| {
                state.borrow_mut().catalog.models.remove(idx);
                state.borrow_mut().dirty = true;
                overlay.add_toast(Toast::new("Model removed."));
                refresh_model_list(&list, &state, &overlay, &parent);
            });
        }

        list.append(&row);
    }
}

fn refresh_lora_list(list: &gtk::ListBox, state: &SharedState, overlay: &ToastOverlay) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for (idx, lora) in state.borrow().catalog.loras.iter().enumerate() {
        let subtitle = lora
            .family
            .as_ref()
            .map(|family| format!("{} • {}", lora.id, family))
            .unwrap_or_else(|| lora.id.clone());

        let row = adw::ActionRow::builder()
            .title(&lora.display_name)
            .subtitle(&subtitle)
            .activatable(false)
            .build();

        let edit_button = gtk::Button::with_label("Edit");
        edit_button.set_halign(Align::Center);
        row.add_suffix(&edit_button);

        let duplicate_button = gtk::Button::with_label("Duplicate");
        duplicate_button.set_halign(Align::Center);
        row.add_suffix(&duplicate_button);

        let delete_button = gtk::Button::with_label("Remove");
        delete_button.set_halign(Align::Center);
        row.add_suffix(&delete_button);

        {
            let state = state.clone();
            let list = list.clone();
            let overlay = overlay.clone();
            edit_button.connect_clicked(move |_| {
                match edit_lora_dialog(Some(idx), &state, &overlay) {
                    Ok(true) => {
                        refresh_lora_list(&list, &state, &overlay);
                    }
                    Ok(false) => {}
                    Err(err) => {
                        overlay.add_toast(Toast::new(&format!("Failed to edit LoRA: {err}")));
                    }
                }
            });
        }

        {
            let state = state.clone();
            let list = list.clone();
            let overlay = overlay.clone();
            duplicate_button.connect_clicked(move |_| {
                let duplicated = {
                    let state_ref = state.borrow();
                    state_ref.catalog.loras.get(idx).cloned().map(|mut lora| {
                        lora.id = next_lora_id(&state_ref.catalog.loras);
                        lora.display_name = format!("{} (Copy)", lora.display_name.trim_end());
                        lora
                    })
                };
                if let Some(lora) = duplicated {
                    let mut state_mut = state.borrow_mut();
                    state_mut.catalog.loras.push(lora);
                    state_mut.dirty = true;
                    overlay.add_toast(Toast::new("LoRA duplicated."));
                    refresh_lora_list(&list, &state, &overlay);
                }
            });
        }

        {
            let state = state.clone();
            let list = list.clone();
            let overlay = overlay.clone();
            delete_button.connect_clicked(move |_| {
                state.borrow_mut().catalog.loras.remove(idx);
                state.borrow_mut().dirty = true;
                overlay.add_toast(Toast::new("LoRA removed."));
                refresh_lora_list(&list, &state, &overlay);
            });
        }

        list.append(&row);
    }
}

fn edit_model_dialog(
    index: Option<usize>,
    state: &SharedState,
    overlay: &ToastOverlay,
    parent: &ApplicationWindow,
) -> Result<()> {
    let mut model = if let Some(idx) = index {
        state.borrow().catalog.models[idx].clone()
    } else {
        MasterModel {
            id: String::new(),
            display_name: String::new(),
            family: String::new(),
            variants: Vec::new(),
        }
    };

    let dialog_title = if index.is_some() {
        "Edit Model"
    } else {
        "Add Model"
    };

    let dialog = gtk::Dialog::builder()
        .title(dialog_title)
        .modal(true)
        .transient_for(parent)
        .default_width(720)
        .default_height(600)
        .build();

    dialog.set_hide_on_close(true);
    dialog.set_destroy_with_parent(true);

    let title_label = gtk::Label::new(Some(dialog_title));
    title_label.add_css_class("title-1");
    let header = HeaderBar::new();
    header.set_title_widget(Some(&title_label));
    header.set_show_end_title_buttons(true);
    dialog.set_titlebar(Some(&header));

    let content = dialog.content_area();
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_spacing(12);

    let form = gtk::Grid::builder()
        .column_spacing(12)
        .row_spacing(12)
        .hexpand(true)
        .build();

    let id_entry = gtk::Entry::new();
    id_entry.set_text(&model.id);
    add_row(&form, 0, "Model ID", &id_entry);

    let display_entry = gtk::Entry::new();
    display_entry.set_text(&model.display_name);
    add_row(&form, 1, "Display Name", &display_entry);

    let family_entry = gtk::Entry::new();
    family_entry.set_text(&model.family);
    add_row(&form, 2, "Family", &family_entry);

    content.append(&form);

    let variants_label = gtk::Label::new(Some("Variants"));
    variants_label.set_halign(Align::Start);
    variants_label.add_css_class("heading");
    content.append(&variants_label);

    let variants_state = Rc::new(RefCell::new(model.variants.clone()));

    let variant_list = gtk::ListBox::new();
    variant_list.set_selection_mode(SelectionMode::None);
    refresh_variant_rows(&variant_list, &variants_state, overlay, &dialog);

    let variant_scroller = gtk::ScrolledWindow::builder()
        .min_content_height(220)
        .child(&variant_list)
        .build();
    content.append(&variant_scroller);

    let add_variant_button = gtk::Button::with_label("Add Variant");
    add_variant_button.set_margin_top(12);
    content.append(&add_variant_button);

    {
        let variants_state = variants_state.clone();
        let variant_list = variant_list.clone();
        let overlay = overlay.clone();
        let dialog = dialog.clone();
        add_variant_button.connect_clicked(move |_| {
            if let Some(variant) = edit_variant_dialog(None, &dialog, overlay.clone()) {
                variants_state.borrow_mut().push(variant);
                refresh_variant_rows(&variant_list, &variants_state, &overlay, &dialog);
            }
        });
    }

    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Save", ResponseType::Ok);

    let response = run_dialog(&dialog);

    if response == ResponseType::Ok {
        let id = id_entry.text().trim().to_string();
        if id.is_empty() {
            overlay.add_toast(Toast::new("Model ID is required."));
            return Ok(());
        }

        let display_name = display_entry.text().trim().to_string();
        if display_name.is_empty() {
            overlay.add_toast(Toast::new("Display name is required."));
            return Ok(());
        }

        let family = family_entry.text().trim().to_string();
        if family.is_empty() {
            overlay.add_toast(Toast::new("Family is required."));
            return Ok(());
        }

        let variants = variants_state.borrow();
        if variants.is_empty() {
            overlay.add_toast(Toast::new("At least one variant is required."));
            return Ok(());
        }

        model.id = id;
        model.display_name = display_name;
        model.family = family;
        model.variants = variants.clone();

        let mut state_mut = state.borrow_mut();
        if let Some(i) = index {
            state_mut.catalog.models[i] = model;
        } else {
            if state_mut.catalog.models.iter().any(|m| m.id == model.id) {
                overlay.add_toast(Toast::new("A model with that ID already exists."));
                return Ok(());
            }
            state_mut.catalog.models.push(model);
        }
        state_mut.dirty = true;
        overlay.add_toast(Toast::new("Model saved."));
    }

    Ok(())
}

fn edit_lora_dialog(
    index: Option<usize>,
    state: &SharedState,
    overlay: &ToastOverlay,
) -> Result<bool> {
    let preset_id = if index.is_some() {
        None
    } else {
        Some(next_lora_id(&state.borrow().catalog.loras))
    };

    let mut definition = if let Some(idx) = index {
        state.borrow().catalog.loras[idx].clone()
    } else {
        LoraDefinition {
            id: String::new(),
            display_name: String::new(),
            family: None,
            download_url: String::new(),
            note: None,
            file_name: None,
        }
    };

    if let Some(new_id) = preset_id {
        definition.id = new_id;
    }

    let dialog_title = if index.is_some() {
        "Edit LoRA"
    } else {
        "Add LoRA"
    };

    let dialog = gtk::Dialog::builder()
        .title(dialog_title)
        .modal(true)
        .default_width(520)
        .default_height(360)
        .build();
    dialog.set_hide_on_close(true);
    dialog.set_destroy_with_parent(true);

    let title_label = gtk::Label::new(Some(dialog_title));
    title_label.add_css_class("title-2");
    let header = HeaderBar::new();
    header.set_title_widget(Some(&title_label));
    header.set_show_end_title_buttons(true);
    dialog.set_titlebar(Some(&header));

    let content = dialog.content_area();
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_spacing(12);

    let form = gtk::Grid::builder()
        .column_spacing(12)
        .row_spacing(12)
        .hexpand(true)
        .build();

    let id_label = gtk::Label::new(Some(&definition.id));
    id_label.set_halign(Align::Start);
    add_row(&form, 0, "LoRA ID", &id_label);

    let name_entry = gtk::Entry::new();
    name_entry.set_text(&definition.display_name);
    add_row(&form, 1, "Display Name", &name_entry);

    let family_entry = gtk::Entry::new();
    if let Some(family) = &definition.family {
        family_entry.set_text(family);
    }
    add_row(&form, 2, "Family (optional)", &family_entry);

    let url_entry = gtk::Entry::new();
    url_entry.set_text(&definition.download_url);
    url_entry.set_placeholder_text(Some("https://civitai.com/api/download/..."));
    add_row(&form, 3, "Download URL", &url_entry);

    let file_entry = gtk::Entry::new();
    if let Some(file) = &definition.file_name {
        file_entry.set_text(file);
    }
    add_row(&form, 4, "Override File Name (optional)", &file_entry);

    content.append(&form);

    let token_hint = gtk::Label::new(Some(
        "For Civitai links, users must save their personal API token in the Arctic Downloader settings.",
    ));
    token_hint.set_halign(Align::Start);
    token_hint.set_wrap(true);
    token_hint.set_margin_top(4);
    content.append(&token_hint);

    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Save", ResponseType::Ok);

    let response = run_dialog(&dialog);
    if response != ResponseType::Ok {
        return Ok(false);
    }

    let display_name = name_entry.text().trim().to_string();
    if display_name.is_empty() {
        overlay.add_toast(Toast::new("LoRA display name is required."));
        return Ok(false);
    }

    let download_url = url_entry.text().trim().to_string();
    if download_url.is_empty() {
        overlay.add_toast(Toast::new("LoRA download URL is required."));
        return Ok(false);
    }

    definition.display_name = display_name;
    definition.download_url = download_url;
    definition.family = entry_to_option(&family_entry);
    definition.file_name = entry_to_option(&file_entry);

    {
        let mut state_mut = state.borrow_mut();
        if let Some(idx) = index {
            state_mut.catalog.loras[idx] = definition;
        } else {
            if state_mut
                .catalog
                .loras
                .iter()
                .any(|l| l.id == definition.id)
            {
                overlay.add_toast(Toast::new("A LoRA with that ID already exists."));
                return Ok(false);
            }
            state_mut.catalog.loras.push(definition);
        }
        state_mut.dirty = true;
    }

    overlay.add_toast(Toast::new("LoRA saved."));
    Ok(true)
}

fn refresh_variant_rows(
    list: &gtk::ListBox,
    variants_state: &Rc<RefCell<Vec<ModelVariant>>>,
    overlay: &ToastOverlay,
    parent: &gtk::Dialog,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for (index, variant) in variants_state.borrow().iter().enumerate() {
        let row = adw::ActionRow::builder()
            .title(&variant.selection_label())
            .subtitle(&format!(
                "Requires ≥ {} GB • {} artifact(s)",
                variant.min_vram_gb,
                variant.artifacts.len()
            ))
            .activatable(false)
            .build();

        let edit_button = gtk::Button::with_label("Edit");
        edit_button.set_halign(Align::Center);
        row.add_suffix(&edit_button);

        let duplicate_button = gtk::Button::with_label("Duplicate");
        duplicate_button.set_halign(Align::Center);
        row.add_suffix(&duplicate_button);

        let delete_button = gtk::Button::with_label("Remove");
        delete_button.set_halign(Align::Center);
        row.add_suffix(&delete_button);

        {
            let variants_state = variants_state.clone();
            let list = list.clone();
            let overlay = overlay.clone();
            let dialog = parent.clone();
            edit_button.connect_clicked(move |_| {
                let existing = variants_state.borrow()[index].clone();
                if let Some(updated) = edit_variant_dialog(Some(existing), &dialog, overlay.clone())
                {
                    variants_state.borrow_mut()[index] = updated;
                    refresh_variant_rows(&list, &variants_state, &overlay, &dialog);
                }
            });
        }

        {
            let variants_state = variants_state.clone();
            let list = list.clone();
            let overlay = overlay.clone();
            let dialog = parent.clone();
            duplicate_button.connect_clicked(move |_| {
                let existing = variants_state.borrow()[index].clone();
                let mut copy = existing.clone();
                copy.id = generate_variant_copy_id(&existing.id, &variants_state.borrow());
                variants_state.borrow_mut().push(copy);
                overlay.add_toast(Toast::new("Variant duplicated."));
                refresh_variant_rows(&list, &variants_state, &overlay, &dialog);
            });
        }

        {
            let variants_state = variants_state.clone();
            let list = list.clone();
            let overlay = overlay.clone();
            let dialog = parent.clone();
            delete_button.connect_clicked(move |_| {
                variants_state.borrow_mut().remove(index);
                overlay.add_toast(Toast::new("Variant removed."));
                refresh_variant_rows(&list, &variants_state, &overlay, &dialog);
            });
        }

        list.append(&row);
    }
}

fn edit_variant_dialog(
    existing: Option<ModelVariant>,
    parent: &gtk::Dialog,
    overlay: ToastOverlay,
) -> Option<ModelVariant> {
    let variant = existing.unwrap_or_else(|| ModelVariant {
        id: String::new(),
        quality_tier: QualityTier::Medium,
        min_vram_gb: 0,
        model_size: None,
        quantization: None,
        note: None,
        artifacts: Vec::new(),
    });

    let dialog = gtk::Dialog::builder()
        .title("Variant")
        .transient_for(parent)
        .modal(true)
        .default_width(720)
        .default_height(600)
        .build();

    dialog.set_hide_on_close(true);
    dialog.set_destroy_with_parent(true);

    let title_label = gtk::Label::new(Some("Variant"));
    title_label.add_css_class("title-2");
    let header = HeaderBar::new();
    header.set_title_widget(Some(&title_label));
    header.set_show_end_title_buttons(true);
    dialog.set_titlebar(Some(&header));

    let content = dialog.content_area();
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_spacing(12);

    let form = gtk::Grid::builder()
        .column_spacing(12)
        .row_spacing(12)
        .hexpand(true)
        .build();

    let id_entry = gtk::Entry::new();
    id_entry.set_text(&variant.id);
    add_row(&form, 0, "Variant ID", &id_entry);

    let min_vram_entry = gtk::Entry::new();
    if variant.min_vram_gb > 0 {
        min_vram_entry.set_text(&variant.min_vram_gb.to_string());
    }
    add_row(&form, 1, "Min VRAM (GB)", &min_vram_entry);

    let quality_combo = gtk::ComboBoxText::new();
    for tier in quality_tiers() {
        let label = tier.label();
        quality_combo.append(Some(label), label);
    }
    quality_combo.set_active_id(Some(variant.quality_tier.label()));
    add_row(&form, 2, "Quality Tier", &quality_combo);

    let size_entry = gtk::Entry::new();
    if let Some(size) = &variant.model_size {
        size_entry.set_text(size);
    }
    add_row(&form, 3, "Model Size", &size_entry);

    let quant_entry = gtk::Entry::new();
    if let Some(quant) = &variant.quantization {
        quant_entry.set_text(quant);
    }
    add_row(&form, 4, "Quantization", &quant_entry);

    let note_entry = gtk::Entry::new();
    if let Some(note) = &variant.note {
        note_entry.set_text(note);
    }
    add_row(&form, 5, "Note", &note_entry);

    content.append(&form);

    let artifacts_label = gtk::Label::new(Some("Artifacts (target_category + full download URL)."));
    artifacts_label.set_halign(Align::Start);
    artifacts_label.set_wrap(true);
    content.append(&artifacts_label);

    let artifact_rows: Rc<RefCell<Vec<ArtifactRow>>> = Rc::new(RefCell::new(Vec::new()));
    let artifacts_list = gtk::Box::new(Orientation::Vertical, 6);
    artifacts_list.set_hexpand(true);

    let artifacts_box = gtk::Box::new(Orientation::Vertical, 6);
    artifacts_box.append(&artifacts_list);

    let add_artifact_button = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Add artifact entry")
        .build();

    content.append(&add_artifact_button);
    content.append(&artifacts_box);

    let add_artifact_row: Rc<dyn Fn(Option<ArtifactRowPreset>)> = {
        let artifacts_list = artifacts_list.clone();
        let artifact_rows = artifact_rows.clone();
        Rc::new(move |preset: Option<ArtifactRowPreset>| {
            let row = gtk::Box::new(Orientation::Horizontal, 6);
            row.set_spacing(6);
            row.set_hexpand(true);

            let target_entry = gtk::Entry::new();
            target_entry.set_placeholder_text(Some("target_category"));
            target_entry.set_width_chars(18);

            let url_entry = gtk::Entry::new();
            url_entry.set_placeholder_text(Some("https://huggingface.co/..."));
            url_entry.set_hexpand(true);

            let remove_button = gtk::Button::builder()
                .icon_name("list-remove-symbolic")
                .tooltip_text("Remove artifact")
                .build();

            row.append(&target_entry);
            row.append(&url_entry);
            row.append(&remove_button);

            let (
                target_slug,
                url_text,
                sha256,
                size_bytes,
                license_url,
                original_repo,
                original_path,
            ) = match preset {
                Some(p) => (
                    p.target_slug,
                    p.url,
                    p.sha256,
                    p.size_bytes,
                    p.license_url,
                    p.repo,
                    p.path,
                ),
                None => (String::new(), String::new(), None, None, None, None, None),
            };

            if !target_slug.is_empty() {
                target_entry.set_text(&target_slug);
            }
            if !url_text.is_empty() {
                url_entry.set_text(&url_text);
            }

            let artifact_row = ArtifactRow {
                container: row.clone(),
                target_entry: target_entry.clone(),
                url_entry: url_entry.clone(),
                sha256,
                size_bytes,
                license_url,
                original_repo,
                original_path,
            };
            artifact_rows.borrow_mut().push(artifact_row);

            let row_clone = row.clone();
            let artifact_rows_remove = artifact_rows.clone();
            remove_button.connect_clicked(move |_| {
                row_clone.unparent();
                let row_ptr = row_clone.as_ptr();
                artifact_rows_remove
                    .borrow_mut()
                    .retain(|item| item.container.as_ptr() != row_ptr);
            });

            artifacts_list.append(&row);
            row.show();
        })
    };

    for artifact in &variant.artifacts {
        let url = build_huggingface_download_url(&artifact.repo, &artifact.path)
            .unwrap_or_else(|| artifact.repo.clone());
        add_artifact_row(Some(ArtifactRowPreset {
            target_slug: artifact.target_category.slug().to_string(),
            url,
            repo: Some(artifact.repo.clone()),
            path: Some(artifact.path.clone()),
            sha256: artifact.sha256.clone(),
            size_bytes: artifact.size_bytes,
            license_url: artifact.license_url.clone(),
        }));
    }

    {
        let add_artifact_row = add_artifact_row.clone();
        add_artifact_button.connect_clicked(move |_| {
            add_artifact_row(None);
        });
    }

    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Save", ResponseType::Ok);

    let response = run_dialog(&dialog);
    if response != ResponseType::Ok {
        return None;
    }

    let id = id_entry.text().trim().to_string();
    if id.is_empty() {
        overlay.add_toast(Toast::new("Variant ID is required."));
        return None;
    }

    let min_vram: u32 = match min_vram_entry.text().trim() {
        text if text.is_empty() => 0,
        text => match text.parse() {
            Ok(value) => value,
            Err(_) => {
                overlay.add_toast(Toast::new(
                    "Minimum VRAM must be a whole number of gigabytes.",
                ));
                return None;
            }
        },
    };

    let quality_tier = quality_combo
        .active_id()
        .and_then(|id| parse_quality_tier(id.as_str()))
        .or_else(|| {
            quality_combo
                .active_text()
                .and_then(|text| parse_quality_tier(text.as_str()))
        });
    let quality_tier = match quality_tier {
        Some(tier) => tier,
        None => {
            overlay.add_toast(Toast::new("Select a quality tier."));
            return None;
        }
    };

    let model_size = entry_to_option(&size_entry);
    let quantization = entry_to_option(&quant_entry);
    let note = entry_to_option(&note_entry);

    let artifact_entries = artifact_rows.borrow();
    let mut artifacts = Vec::new();
    if artifact_entries.is_empty() {
        overlay.add_toast(Toast::new("Add at least one artifact for this variant."));
        return None;
    }

    for row in artifact_entries.iter() {
        let target_slug = row.target_entry.text().trim().to_ascii_lowercase();
        if target_slug.is_empty() {
            overlay.add_toast(Toast::new("Artifact target_category is required."));
            return None;
        }

        let target_category = match TargetCategory::from_slug(&target_slug) {
            Some(category) => category,
            None => {
                overlay.add_toast(Toast::new(&format!(
                    "Unknown target category: {target_slug}"
                )));
                return None;
            }
        };

        let url_text = row.url_entry.text().trim().to_string();
        if url_text.is_empty() {
            overlay.add_toast(Toast::new("Artifact download URL is required."));
            return None;
        }

        let parsed = parse_huggingface_download_url(&url_text).or_else(|err| {
            if let (Some(repo), Some(path)) = (&row.original_repo, &row.original_path) {
                if url_text == *repo {
                    Ok((repo.clone(), path.clone()))
                } else {
                    Err(err)
                }
            } else {
                Err(err)
            }
        });

        let (repo, path) = match parsed {
            Ok(parts) => parts,
            Err(err) => {
                overlay.add_toast(Toast::new(&format!("Invalid Hugging Face URL: {err}")));
                return None;
            }
        };

        artifacts.push(ModelArtifact {
            repo,
            path,
            sha256: row.sha256.clone(),
            size_bytes: row.size_bytes,
            target_category,
            license_url: row.license_url.clone(),
            direct_url: Some(url_text),
        });
    }

    Some(ModelVariant {
        id,
        quality_tier,
        min_vram_gb: min_vram,
        model_size,
        quantization,
        note,
        artifacts,
    })
}

fn quality_tiers() -> [QualityTier; 4] {
    [
        QualityTier::Ultra,
        QualityTier::High,
        QualityTier::Medium,
        QualityTier::Low,
    ]
}

fn parse_quality_tier(label: &str) -> Option<QualityTier> {
    match label {
        "Ultra" => Some(QualityTier::Ultra),
        "High" => Some(QualityTier::High),
        "Medium" => Some(QualityTier::Medium),
        "Low" => Some(QualityTier::Low),
        _ => None,
    }
}

fn entry_to_option(entry: &gtk::Entry) -> Option<String> {
    let text = entry.text().trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn generate_model_copy_id(base: &str, models: &[MasterModel]) -> String {
    let mut suffix = 1;
    let mut candidate = format!("{base}-copy");
    while models.iter().any(|model| model.id == candidate) {
        suffix += 1;
        candidate = format!("{base}-copy-{suffix}");
    }
    candidate
}

fn generate_variant_copy_id(base: &str, variants: &[ModelVariant]) -> String {
    let mut suffix = 1;
    let mut candidate = format!("{base}-copy");
    while variants.iter().any(|variant| variant.id == candidate) {
        suffix += 1;
        candidate = format!("{base}-copy-{suffix}");
    }
    candidate
}

fn next_lora_id(loras: &[LoraDefinition]) -> String {
    let mut max_id: u32 = 0;
    for lora in loras {
        if let Ok(value) = lora.id.parse::<u32>() {
            if value > max_id {
                max_id = value;
            }
        }
    }
    (max_id + 1).to_string()
}

fn run_dialog(dialog: &gtk::Dialog) -> ResponseType {
    let main_loop = Rc::new(gtk::glib::MainLoop::new(None, false));
    let response = Rc::new(Cell::new(ResponseType::None));

    let main_loop_clone = main_loop.clone();
    let response_clone = response.clone();
    let handler_id = dialog.connect_response(move |dlg, resp| {
        response_clone.set(resp);
        dlg.hide();
        main_loop_clone.quit();
    });

    dialog.present();
    main_loop.run();
    dialog.disconnect(handler_id);

    response.get()
}

fn add_row<W: gtk::prelude::IsA<gtk::Widget>>(grid: &gtk::Grid, row: i32, label: &str, widget: &W) {
    let label_widget = gtk::Label::new(Some(label));
    label_widget.set_halign(Align::End);
    label_widget.set_valign(Align::Center);
    grid.attach(&label_widget, 0, row, 1, 1);

    widget.set_hexpand(true);
    grid.attach(widget, 1, row, 1, 1);
}

fn build_huggingface_download_url(repo: &str, path: &str) -> Option<String> {
    if let Some(rest) = repo.strip_prefix("hf://") {
        let mut parts = rest.split('@');
        let repo_path = parts.next()?;
        let revision = parts.next().unwrap_or("main");
        return Some(format!(
            "https://huggingface.co/{repo_path}/resolve/{revision}/{path}?download=1"
        ));
    }

    if let Some(blob_index) = repo.find("/blob/") {
        let (base, remainder) = repo.split_at(blob_index);
        let remainder = &remainder["/blob/".len()..];
        let mut segments = remainder.splitn(2, '/');
        let revision = segments.next().unwrap_or("main");
        let file_path = segments.next().unwrap_or(path);
        let repo_path = base.trim_start_matches("https://huggingface.co/");
        return Some(format!(
            "https://huggingface.co/{repo_path}/resolve/{revision}/{file_path}?download=1"
        ));
    }

    if repo.starts_with("https://") {
        return Some(format!("{repo}/resolve/main/{path}?download=1"));
    }

    None
}

fn parse_huggingface_download_url(url: &str) -> Result<(String, String)> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("URL is empty"));
    }

    let without_query = trimmed.split('?').next().unwrap_or(trimmed);

    let rest = without_query
        .strip_prefix("https://huggingface.co/")
        .ok_or_else(|| anyhow!("URL must start with https://huggingface.co/"))?;

    let segments: Vec<&str> = rest
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() < 4 {
        return Err(anyhow!(
            "URL must include /resolve/<revision>/<file path> segments"
        ));
    }

    let owner = segments[0];
    let repo = segments[1];
    let mode = segments[2];

    if mode != "resolve" && mode != "blob" {
        return Err(anyhow!(
            "URL must contain /resolve/<revision>/… or /blob/<revision>/…"
        ));
    }

    if segments.len() < 5 {
        return Err(anyhow!("URL is missing the file path segment"));
    }

    let revision = segments[3];
    let path = segments[4..].join("/");
    let repo_string = format!("hf://{owner}/{repo}@{revision}");
    Ok((repo_string, path))
}
