use shiny_counter::counter::{CounterEvent, CounterState};
use shiny_counter::types::Color;

#[test]
fn simulated_shiny_hunt_session() {
    let target = vec![
        Color::new(220, 230, 255),
        Color::new(255, 215, 0),
        Color::new(120, 60, 200),
    ];
    let tolerance = 12;

    let frames: Vec<Vec<Color>> = vec![
        vec![
            Color::new(40, 40, 40),
            Color::new(35, 35, 35),
            Color::new(50, 50, 50),
        ],
        vec![
            Color::new(222, 232, 254),
            Color::new(255, 213, 1),
            Color::new(118, 62, 198),
        ],
        vec![
            Color::new(221, 231, 254),
            Color::new(253, 215, 2),
            Color::new(121, 60, 199),
        ],
        vec![
            Color::new(10, 10, 10),
            Color::new(10, 10, 10),
            Color::new(120, 60, 200),
        ],
        vec![
            Color::new(0, 0, 0),
            Color::new(5, 5, 5),
            Color::new(8, 8, 8),
        ],
        vec![
            Color::new(33, 33, 33),
            Color::new(44, 44, 44),
            Color::new(55, 55, 55),
        ],
        vec![
            Color::new(220, 230, 255),
            Color::new(255, 215, 0),
            Color::new(120, 60, 200),
        ],
        vec![
            Color::new(220, 230, 255),
            Color::new(255, 215, 0),
            Color::new(120, 60, 200),
        ],
        vec![
            Color::new(0, 0, 0),
            Color::new(0, 0, 0),
            Color::new(0, 0, 0),
        ],
        vec![
            Color::new(220, 230, 255),
            Color::new(255, 215, 0),
            Color::new(120, 60, 200),
        ],
    ];

    let mut state = CounterState::default();
    let mut count: u32 = 0;
    let mut increments = 0;
    let mut rearms = 0;
    for frame in &frames {
        match state.tick(frame, &target, tolerance, &mut count) {
            CounterEvent::Incremented => increments += 1,
            CounterEvent::Armed => rearms += 1,
            CounterEvent::None => {}
        }
    }
    assert_eq!(count, 3);
    assert_eq!(increments, 3);
    assert_eq!(rearms, 2);
}

#[test]
fn lingering_on_shiny_does_not_inflate_count() {
    let target = vec![Color::new(10, 10, 10); 3];
    let mut state = CounterState::default();
    let mut count = 0;
    for _ in 0..50 {
        state.tick(&target, &target, 0, &mut count);
    }
    assert_eq!(count, 1);
}
