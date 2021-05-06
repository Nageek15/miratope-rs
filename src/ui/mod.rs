//! Contains the basic code that configures the UI.

pub mod camera;
pub mod library;

use std::{marker::PhantomData, path::PathBuf};

use crate::{
    geometry::{Hyperplane, Point},
    lang::{Options, SelectedLanguage},
    polytope::{concrete::Concrete, Polytope},
    Float, OffOptions,
};
use camera::ProjectionType;
use library::Library;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContext, EguiSettings};
use rfd::FileDialog;
use strum::IntoEnumIterator;

/// Guarantees that file dialogs will be opened on the main thread, used to
/// circumvent a MacOS limitation that all GUI operations must be done on the
/// main thread.
pub struct MainThreadToken(PhantomData<*const ()>);

impl MainThreadToken {
    /// Initializes a new token.
    pub fn new() -> Self {
        Self(Default::default())
    }

    /// Auxiliary function to create a new file dialog.
    fn new_file_dialog() -> FileDialog {
        FileDialog::new()
            .add_filter("OFF File", &["off"])
            .add_filter("GGB file", &["ggb"])
    }

    /// Returns the path given by an open file dialog.
    fn pick_file(&self) -> Option<PathBuf> {
        Self::new_file_dialog().pick_file()
    }

    /// Returns the path given by a save file dialog.
    fn save_file(&self, name: &str) -> Option<PathBuf> {
        Self::new_file_dialog().set_file_name(name).save_file()
    }
}

enum FileDialogMode {
    Disabled,
    Open,
    Save,
}

impl Default for FileDialogMode {
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Default)]
pub struct FileDialogState {
    mode: FileDialogMode,
    name: Option<String>,
}

impl FileDialogState {
    pub fn open(&mut self) {
        self.mode = FileDialogMode::Open;
    }

    pub fn save(&mut self, name: String) {
        self.mode = FileDialogMode::Save;
        self.name = Some(name);
    }
}

/// Stores the state of the cross-section view.
pub enum SectionState {
    /// The view is active.
    Active {
        /// The polytope from which the cross-section originates.
        original_polytope: Concrete,

        /// The range of the slider.
        minmax: (Float, Float),

        /// The position of the slicing hyperplane.
        hyperplane_pos: Float,

        /// Whether the cross-section is flattened into a dimension lower.
        flatten: bool,
    },

    /// The view is inactive.
    Inactive,
}

impl SectionState {
    pub fn reset(&mut self) {
        *self = Self::Inactive;
    }

    pub fn set_pos(&mut self, pos: Float) {
        if let Self::Active {
            original_polytope: _,
            minmax: _,
            hyperplane_pos,
            flatten: _,
        } = self
        {
            *hyperplane_pos = pos;
        }
    }

    pub fn set_flat(&mut self, flat: bool) {
        if let Self::Active {
            original_polytope: _,
            minmax: _,
            hyperplane_pos: _,
            flatten,
        } = self
        {
            *flatten = flat;
        }
    }
}

impl Default for SectionState {
    fn default() -> Self {
        Self::Inactive
    }
}

/// The system in charge of the UI.
pub fn ui(
    egui_ctx: ResMut<EguiContext>,
    mut query: Query<&mut Concrete>,
    mut section_state: ResMut<SectionState>,
    mut file_dialog_state: ResMut<FileDialogState>,
    mut projection_type: ResMut<ProjectionType>,
    mut library: ResMut<Library>,
    mut selected_language: ResMut<SelectedLanguage>,
) {
    let ctx = egui_ctx.ctx();

    // The top bar.
    egui::TopPanel::top("top_panel").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            // Operations on files.
            egui::menu::menu(ui, "File", |ui| {
                // Loads a file.
                if ui.button("Open").clicked() {
                    file_dialog_state.open();
                }

                // Saves a file.
                if ui.button("Save").clicked() {
                    if let Some(p) = query.iter_mut().next() {
                        file_dialog_state
                            .save(selected_language.parse_uppercase(p.name(), Default::default()));
                    }
                }

                ui.separator();

                // Quits the application.
                if ui.button("Exit").clicked() {
                    std::process::exit(0);
                }
            });

            // Configures the view.
            egui::menu::menu(ui, "View", |ui| {
                let mut checked = projection_type.is_orthogonal();

                if ui.checkbox(&mut checked, "Orthogonal projection").clicked() {
                    projection_type.flip();

                    // Forces an update on all polytopes. (This does have an effect!)
                    for mut p in query.iter_mut() {
                        // We'll use this code once Clippy Issue #7171 is fixed:
                        // #[allow(clippy::no_effect)]
                        // &mut *p;

                        // Workaround:
                        p.name_mut();
                    }
                }
            });

            // Operations on polytopes.
            egui::menu::menu(ui, "Polytope", |ui| {
                ui.collapsing("Operations", |ui| {
                    // Converts the active polytope into its dual.
                    if ui.button("Dual").clicked() {
                        for mut p in query.iter_mut() {
                            match p.dual_mut() {
                                Ok(_) => println!("Dual succeeded."),
                                Err(idx) => println!(
                                    "Dual failed: Facet {} passes through inversion center.",
                                    idx
                                ),
                            }

                            section_state.reset();
                        }
                    }

                    ui.separator();

                    // Makes a pyramid out of the current polytope.
                    if ui.button("Pyramid").clicked() {
                        for mut p in query.iter_mut() {
                            *p = p.pyramid();
                        }

                        section_state.reset();
                    }

                    // Makes a prism out of the current polytope.
                    if ui.button("Prism").clicked() {
                        for mut p in query.iter_mut() {
                            *p = p.prism();
                        }

                        section_state.reset();
                    }

                    // Makes a tegum out of the current polytope.
                    if ui.button("Tegum").clicked() {
                        for mut p in query.iter_mut() {
                            *p = p.tegum();
                        }
                    }

                    ui.separator();

                    // Recenters a polytope.
                    if ui.button("Recenter").clicked() {
                        for mut p in query.iter_mut() {
                            p.recenter();
                        }

                        section_state.reset();
                    }

                    ui.separator();

                    // Toggles cross-section mode.
                    if ui.button("Cross-section").clicked() {
                        *section_state = match *section_state {
                            SectionState::Active {
                                original_polytope: _,
                                minmax: _,
                                hyperplane_pos: _,
                                flatten: _,
                            } => SectionState::Inactive,
                            SectionState::Inactive => {
                                let p = query.iter_mut().next().unwrap();
                                let minmax = p.x_minmax().unwrap_or((-1.0, 1.0));

                                SectionState::Active {
                                    original_polytope: p.clone(),
                                    minmax,
                                    hyperplane_pos: (minmax.0 + minmax.1) / 2.0,
                                    flatten: false,
                                }
                            }
                        };
                    }
                });

                ui.collapsing("Elements", |ui| {
                    // Converts the active polytope into any of its facets.
                    if ui.button("Facet").clicked() {
                        for mut p in query.iter_mut() {
                            println!("Facet");

                            if let Some(mut facet) = p.facet(0) {
                                facet.flatten();
                                facet.recenter();

                                *p = facet;
                                println!("Facet succeeded.")
                            } else {
                                println!("Facet failed.")
                            }

                            section_state.reset();
                        }
                    }

                    // Converts the active polytope into any of its verfs.
                    if ui.button("Verf").clicked() {
                        for mut p in query.iter_mut() {
                            println!("Verf");

                            if let Some(mut verf) = p.verf(0) {
                                verf.flatten();
                                verf.recenter();
                                *p = verf;

                                println!("Verf succeeded.")
                            } else {
                                println!("Verf failed.")
                            }

                            section_state.reset();
                        }
                    }
                });

                ui.collapsing("Properties", |ui| {
                    // Determines whether the polytope is orientable.
                    if ui.button("Orientability").clicked() {
                        for p in query.iter_mut() {
                            if p.orientable() {
                                println!("The polytope is orientable.");
                            } else {
                                println!("The polytope is not orientable.");
                            }
                        }
                    }

                    // Gets the volume of the polytope.
                    if ui.button("Volume").clicked() {
                        for p in query.iter_mut() {
                            if let Some(vol) = p.volume() {
                                println!("The volume is {}.", vol);
                            } else {
                                println!("The polytope has no volume.");
                            }
                        }
                    }
                });
            });

            // Stuff related to the Polytope Wiki.
            egui::menu::menu(ui, "Wiki", |ui| {
                // Goes to the wiki main page.
                if ui.button("Main Page").clicked() && webbrowser::open(crate::WIKI_LINK).is_err() {
                    println!("Website opening failed!")
                }

                // Searches the current polytope on the wiki.
                if ui.button("Current").clicked() {
                    for p in query.iter_mut() {
                        if webbrowser::open(&p.wiki_link()).is_err() {
                            println!("Website opening failed!")
                        }
                    }
                }
            });

            // Switch language.
            egui::menu::menu(ui, "Language", |ui| {
                for lang in SelectedLanguage::iter() {
                    if ui.button(lang.to_string()).clicked() {
                        *selected_language = lang;
                    }
                }
            });
        });

        // The cross-section settings.
        if let SectionState::Active {
            original_polytope: _,
            minmax,
            hyperplane_pos,
            flatten,
        } = *section_state
        {
            ui.label("Cross section settings:");

            // Sets the slider range to the range of x coordinates in the polytope.
            let mut new_hyperplane_pos = hyperplane_pos;
            ui.add(
                egui::Slider::new(
                    &mut new_hyperplane_pos,
                    (minmax.0 + 0.00001)..=(minmax.1 - 0.00001),
                )
                .text("Slice depth"),
            );

            // Updates the slicing depth.
            #[allow(clippy::float_cmp)]
            if hyperplane_pos != new_hyperplane_pos {
                section_state.set_pos(new_hyperplane_pos);
            }

            ui.horizontal(|ui| {
                // Makes the current cross-section into the main polytope.
                if ui.button("Make main").clicked() {
                    section_state.reset();
                }

                let mut new_flatten = flatten;
                ui.add(egui::Checkbox::new(&mut new_flatten, "Flatten"));

                // Updates the flattening setting.
                if flatten != new_flatten {
                    section_state.set_flat(new_flatten);
                }
            });
        }
    });

    egui::SidePanel::left("side_panel", 300.0).show(ctx, |ui| {
        if let Some(file) = library.show(ui) {
            if let Some(mut p) = query.iter_mut().next() {
                if let Ok(q) = Concrete::from_path(&file) {
                    *p = q;

                    section_state.reset();
                } else {
                    println!("File open failed!");
                }
            }
        }
    });
}

pub fn file_dialog(
    mut query: Query<&mut Concrete>,
    file_dialog_state: ResMut<FileDialogState>,
    mut section_state: ResMut<SectionState>,
    token: NonSend<MainThreadToken>,
) {
    if file_dialog_state.is_changed() {
        match file_dialog_state.mode {
            FileDialogMode::Save => {
                if let Some(path) = token.save_file(file_dialog_state.name.as_ref().unwrap()) {
                    for p in query.iter_mut() {
                        std::fs::write(path.clone(), p.to_off(OffOptions::default())).unwrap();
                    }
                }
            }
            FileDialogMode::Open => {
                if let Some(path) = token.pick_file() {
                    for mut p in query.iter_mut() {
                        *p = Concrete::from_path(&path).unwrap();
                        p.recenter();
                    }

                    section_state.reset();
                }
            }
            _ => {}
        }
    }
}

/// Resizes the UI when the screen is resized.
pub fn update_scale_factor(mut egui_settings: ResMut<EguiSettings>, windows: Res<Windows>) {
    if let Some(window) = windows.get_primary() {
        egui_settings.scale_factor = 1.0 / window.scale_factor();
    }
}

/// Updates polytopes after an operation.
pub fn update_changed_polytopes(
    mut meshes: ResMut<Assets<Mesh>>,
    polies: Query<(&Concrete, &Handle<Mesh>, &Children), Changed<Concrete>>,
    wfs: Query<&Handle<Mesh>, Without<Concrete>>,
    mut windows: ResMut<Windows>,
    selected_language: Res<SelectedLanguage>,
    orthogonal: Res<ProjectionType>,
) {
    for (poly, mesh_handle, children) in polies.iter() {
        *meshes.get_mut(mesh_handle).unwrap() = poly.get_mesh(*orthogonal);

        windows
            .get_primary_mut()
            .unwrap()
            .set_title(selected_language.parse_uppercase(poly.name(), Options::default()));

        for child in children.iter() {
            if let Ok(wf_handle) = wfs.get_component::<Handle<Mesh>>(*child) {
                let wf: &mut Mesh = meshes.get_mut(wf_handle).unwrap();
                *wf = poly.get_wireframe(*orthogonal);

                break;
            }
        }
    }
}

/// Updates the cross-section shown.
pub fn update_cross_section(mut query: Query<&mut Concrete>, state: Res<SectionState>) {
    if state.is_changed() {
        if let SectionState::Active {
            original_polytope,
            hyperplane_pos,
            minmax: _,
            flatten,
        } = &*state
        {
            for mut p in query.iter_mut() {
                let r = original_polytope.clone();
                let hyp_pos = hyperplane_pos + 0.0000001; // Botch fix for degeneracies.

                if let Some(dim) = r.dim() {
                    let hyperplane = Hyperplane::x(dim, hyp_pos);
                    let mut slice = r.slice(&hyperplane);

                    if *flatten {
                        slice.flatten_into(&hyperplane.subspace);
                        slice.recenter_with(
                            &hyperplane.flatten(&hyperplane.project(&Point::zeros(dim))),
                        );
                    }

                    *p = slice;
                }
            }
        }
    }
}

pub fn update_language(
    mut polies: Query<&Concrete>,
    mut windows: ResMut<Windows>,
    selected_language: Res<SelectedLanguage>,
) {
    if selected_language.is_changed() {
        if let Some(poly) = polies.iter_mut().next() {
            windows
                .get_primary_mut()
                .unwrap()
                .set_title(selected_language.parse(poly.name(), Options::default()));
        }
    }
}
