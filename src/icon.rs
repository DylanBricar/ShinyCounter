//! Procedurally generated Pokéball icon for the window title-bar and taskbar.

const SIZE: u32 = 64;

pub fn rgba() -> Vec<u8> {
    let mut out = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let cx = SIZE as f32 / 2.0;
    let cy = SIZE as f32 / 2.0;
    let r_outer = SIZE as f32 / 2.0 - 0.5;
    let r_button_outer = 9.0;
    let r_button_inner = 4.5;
    let band_half_h = 3.5;

    let red = [232u8, 65, 65, 255];
    let white = [248u8, 248, 248, 255];
    let black = [22u8, 22, 22, 255];
    let transparent = [0u8, 0, 0, 0];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let r = (dx * dx + dy * dy).sqrt();

            let p = if r > r_outer + 1.0 {
                transparent
            } else if r > r_outer - 1.5 {
                // Anti-aliased outer ring (just paint solid black for crispness).
                black
            } else if r <= r_button_outer + 0.5 {
                if r <= r_button_inner {
                    white
                } else if r <= r_button_inner + 1.0 {
                    black
                } else if r <= r_button_outer {
                    white
                } else {
                    black
                }
            } else if dy.abs() <= band_half_h {
                black
            } else if dy < 0.0 {
                red
            } else {
                white
            };
            out.extend_from_slice(&p);
        }
    }
    out
}

pub fn size() -> u32 {
    SIZE
}
