//! A wlroots virtual pointer for the headless sway, which has no input
//! devices. Reads orders from stdin: `mover X Y`, `pulsar`, `soltar`, `rueda N`.

#[cfg(target_os = "linux")]
fn main() {
    use std::io::BufRead;
    use std::time::Instant;
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::{wl_pointer, wl_registry};
    use wayland_client::{Connection, Dispatch, QueueHandle};
    use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1 as Gestor;
    use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1 as Puntero;

    struct S;
    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for S {
        fn event(_: &mut S, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<S>) {}
    }
    impl Dispatch<Gestor, ()> for S {
        fn event(_: &mut S, _: &Gestor, _: <Gestor as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<S>) {}
    }
    impl Dispatch<Puntero, ()> for S {
        fn event(_: &mut S, _: &Puntero, _: <Puntero as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<S>) {}
    }

    let conn = Connection::connect_to_env().expect("wayland");
    let (globals, mut cola) = registry_queue_init::<S>(&conn).expect("registro");
    let qh = cola.handle();
    let gestor: Gestor = globals.bind(&qh, 1..=2, ()).expect("zwlr_virtual_pointer_manager_v1");
    let p = gestor.create_virtual_pointer(None, &qh, ());
    let (w, h) = (1920u32, 1080u32);
    let t0 = Instant::now();
    cola.roundtrip(&mut S).unwrap();
    for linea in std::io::stdin().lock().lines() {
        let Ok(linea) = linea else { break };
        let ms = t0.elapsed().as_millis() as u32;
        let partes: Vec<&str> = linea.split_whitespace().collect();
        match partes.as_slice() {
            ["mover", x, y] => p.motion_absolute(ms, x.parse().unwrap(), y.parse().unwrap(), w, h),
            ["pulsar"] => p.button(ms, 0x110, wl_pointer::ButtonState::Pressed),
            ["soltar"] => p.button(ms, 0x110, wl_pointer::ButtonState::Released),
            ["rueda", n] => {
                let n: i32 = n.parse().unwrap();
                p.axis_source(wl_pointer::AxisSource::Wheel);
                p.axis_discrete(ms, wl_pointer::Axis::VerticalScroll, 15.0 * n as f64, n);
            }
            _ => continue,
        }
        p.frame();
        cola.roundtrip(&mut S).unwrap();
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {}
