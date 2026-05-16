use egui::{Color32, Rect, Sense, Stroke, Ui};

const STRIPE_W: f32 = 6.0;
const SHIMMER_SPEED: f32 = 18.0; // pixels per second
const BG: Color32 = Color32::from_rgb(0x22, 0x22, 0x2A);
const FG: Color32 = Color32::from_rgb(0x36, 0x36, 0x40);

/// Draw an animated diagonal-stripe shimmer to mark "computing" regions.
pub fn draw(ui: &mut Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, BG);

    let t = ui.input(|i| i.time) as f32;
    let phase = (t * SHIMMER_SPEED) % (STRIPE_W * 2.0);
    let stripe_count =
        ((rect.width() + rect.height()) / (STRIPE_W * 2.0)).ceil() as i32 + 4;

    for i in -2..stripe_count {
        let x0 = rect.left() + i as f32 * STRIPE_W * 2.0 + phase;
        let pts = [
            egui::pos2(x0, rect.top()),
            egui::pos2(x0 + STRIPE_W, rect.top()),
            egui::pos2(x0 + STRIPE_W + rect.height(), rect.bottom()),
            egui::pos2(x0 + rect.height(), rect.bottom()),
        ];
        painter.add(egui::Shape::convex_polygon(
            pts.to_vec(),
            FG,
            Stroke::NONE,
        ));
    }

    // Need to allocate space so layout works; we already painted, so the
    // response is just a hover sink for siblings.
    let _ = ui.allocate_rect(rect, Sense::hover());
}
