//! Ventanas flotantes que también pueden separarse del programa como ventanas independientes del
//! sistema (por ejemplo en otro monitor) y volver a acoplarse. Mixer, biblioteca, plugins, ruteo y
//! video usan este mismo mecanismo; las ventanas separadas se recuerdan entre sesiones.
use crate::*;
use egui::{RichText, vec2};

impl App {
    fn separada(&self, id: &str) -> bool {
        self.config.separadas.iter().any(|s| s == id)
    }

    /// Separa o acopla una ventana y lo recuerda.
    pub fn alternar_separada(&mut self, id: &str) {
        if self.separada(id) {
            self.config.separadas.retain(|s| s != id);
        } else {
            self.config.separadas.push(id.to_string());
        }
        self.config.save();
    }

    /// Muestra la ventana `id`: flotante dentro del programa o separada como ventana del sistema.
    /// Devuelve `false` si se cerró. `scroll` envuelve el contenido en una zona desplazable.
    pub fn ventana(&mut self, ctx: &egui::Context, id: &'static str, titulo: String, tam: [f32; 2], scroll: bool, contenido: impl FnOnce(&mut Self, &mut egui::Ui)) -> bool {
        let mut contenido = Some(contenido);
        let (mut abierta, mut alternar) = (true, false);
        if self.separada(id) {
            let mut builder = egui::ViewportBuilder::default().with_title(format!("{titulo} — Quantum DAW")).with_inner_size(tam).with_min_inner_size([240.0, 160.0]).with_app_id("quantum-daw");
            if let Some(icono) = logo_icon() {
                builder = builder.with_icon(Arc::new(icono));
            }
            ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(("ventana", id)), builder, |ui, _| {
                egui::Panel::top(egui::Id::new(("barra-ventana", id))).frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(10, 6))).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&titulo).strong().size(14.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let b = egui::Button::new(RichText::new(tr("Acoplar al programa")).color(BG).strong()).fill(ACCENT);
                            if ui.add(b).on_hover_text(tr("Vuelve a mostrar esta ventana dentro de Quantum DAW")).clicked() {
                                alternar = true;
                            }
                        });
                    });
                });
                egui::CentralPanel::default().frame(egui::Frame::new().fill(PANEL).inner_margin(10.0)).show(ui, |ui| {
                    if let Some(f) = contenido.take() {
                        if scroll {
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| f(self, ui));
                        } else {
                            f(self, ui);
                        }
                    }
                });
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    abierta = false;
                }
            });
            // Las ventanas separadas se dibujan junto con la principal: que la principal siga refrescando.
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        } else {
            let mut ventana = egui::Window::new(RichText::new(&titulo).strong()).id(egui::Id::new(id)).open(&mut abierta).default_size(vec2(tam[0], tam[1]));
            if scroll {
                ventana = ventana.vscroll(true);
            }
            ventana.show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(tr("Separar ventana")).on_hover_text(tr("Abre esta ventana fuera del programa (por ejemplo en otro monitor)")).clicked() {
                            alternar = true;
                        }
                    });
                });
                if let Some(f) = contenido.take() {
                    f(self, ui);
                }
            });
        }
        if alternar {
            self.alternar_separada(id);
        }
        abierta
    }
}
