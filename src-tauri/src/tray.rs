//! Ícone na bandeja do Windows: estado do backup automático e menu rápido.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::auto::AutoStatusView;
use crate::state::AppState;

pub const TRAY_ID: &str = "principal";
/// Centro do ponto de estado no ícone.
pub const DOT_X: u32 = 23;
pub const DOT_Y: u32 = 23;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Protegido,
    EmRisco,
    Copiando,
    Pausado,
}

pub fn tray_state(s: &AutoStatusView) -> TrayState {
    if s.paused {
        TrayState::Pausado
    } else if s.copying {
        TrayState::Copiando
    } else if s.at_risk > 0 {
        TrayState::EmRisco
    } else {
        TrayState::Protegido
    }
}

/// Ícone 32×32 desenhado no código: escudo verde com um ponto da cor do estado.
pub fn tray_icon_rgba(state: TrayState) -> (Vec<u8>, u32, u32) {
    const W: u32 = 32;
    let dot: [u8; 4] = match state {
        TrayState::Protegido => [0x19, 0xe6, 0xa0, 0xff],
        TrayState::EmRisco => [0xff, 0x5a, 0x5f, 0xff],
        TrayState::Copiando => [0x4d, 0x9f, 0xff, 0xff],
        TrayState::Pausado => [0x9a, 0xa5, 0xa0, 0xff],
    };
    let shield = [0x13, 0x8a, 0x66, 0xff];
    let edge = [0x0b, 0x10, 0x0e, 0xff];
    let mut v = vec![0u8; (W * W * 4) as usize];
    let mut put = |x: u32, y: u32, c: [u8; 4]| {
        let i = ((y * W + x) * 4) as usize;
        v[i..i + 4].copy_from_slice(&c);
    };
    for y in 2..30u32 {
        // escudo: reto em cima, afinando até a ponta embaixo
        let half = if y < 17 {
            12
        } else {
            12u32.saturating_sub((y - 17) * 12 / 13)
        };
        for x in 16u32.saturating_sub(half)..(16 + half).min(W) {
            put(x, y, shield);
        }
    }
    for y in 0..W {
        for x in 0..W {
            let (dx, dy) = (x as i32 - DOT_X as i32, y as i32 - DOT_Y as i32);
            let d2 = dx * dx + dy * dy;
            if d2 <= 49 {
                put(x, y, if d2 >= 36 { edge } else { dot });
            }
        }
    }
    (v, W, W)
}

pub fn tray_tooltip(s: &AutoStatusView) -> String {
    let base = "SaveGuard — ";
    if s.paused {
        return format!("{base}backup automático pausado");
    }
    if s.copying {
        return format!("{base}copiando saves…");
    }
    if let Some(jogo) = s.running.first() {
        return format!("{base}jogando {jogo} (copio quando fechar)");
    }
    match s.at_risk {
        0 => format!("{base}protegido"),
        1 => format!("{base}1 jogo sem cópia fora do PC"),
        n => format!("{base}{n} jogos sem cópia fora do PC"),
    }
}

/// Mostra e traz a janela principal para a frente.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn menu(app: &AppHandle, s: &AutoStatusView) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let plugged = state
        .plugged
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let m = Menu::new(app)?;
    m.append(&MenuItem::with_id(
        app,
        "backup_agora",
        "Fazer backup agora",
        true,
        None::<&str>,
    )?)?;
    m.append(&MenuItem::with_id(
        app,
        "abrir",
        "Abrir SaveGuard",
        true,
        None::<&str>,
    )?)?;
    let pausa = if s.paused {
        "Retomar backup automático"
    } else {
        "Pausar backup automático"
    };
    m.append(&MenuItem::with_id(
        app,
        "pausar",
        pausa,
        true,
        None::<&str>,
    )?)?;
    if !plugged.is_empty() {
        m.append(&PredefinedMenuItem::separator(app)?)?;
        for d in &plugged {
            let id = format!("copiar:{}", d.id);
            let texto = format!("Copiar tudo para {}", d.label);
            m.append(&MenuItem::with_id(app, id, texto, true, None::<&str>)?)?;
        }
    }
    m.append(&PredefinedMenuItem::separator(app)?)?;
    m.append(&MenuItem::with_id(app, "sair", "Sair", true, None::<&str>)?)?;
    Ok(m)
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "abrir" => show_main(app),
        "sair" => app.exit(0),
        "pausar" => {
            let state = app.state::<AppState>();
            let paused = !state.config().auto_paused;
            if let Err(e) = state.update_config(|c| c.auto_paused = paused) {
                eprintln!("bandeja: {e}");
            }
            crate::auto::publish(app, |s| s.paused = paused);
        }
        "backup_agora" => {
            let app = app.clone();
            std::thread::spawn(move || crate::auto::backup_all_now(&app));
        }
        other => {
            if let Some(dest) = other.strip_prefix("copiar:") {
                let (app, dest) = (app.clone(), dest.to_string());
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let r = crate::commands::copy_all_blocking(&state, &dest);
                    let erros = r.iter().filter(|x| x.status == "erro").count();
                    let msg = if erros == 0 {
                        "Todos os saves foram copiados.".to_string()
                    } else {
                        format!(
                            "{erros} jogo(s) não puderam ser copiados. Abra o SaveGuard para ver."
                        )
                    };
                    crate::notify_user(&app, "Cópia para o disco", &msg);
                });
            }
        }
    }
}

/// Cria o ícone na bandeja.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let s = AutoStatusView::default();
    let (rgba, w, h) = tray_icon_rgba(tray_state(&s));
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::new_owned(rgba, w, h))
        .tooltip(tray_tooltip(&s))
        .menu(&menu(app, &s)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Atualiza ícone, dica e menu conforme o estado.
pub fn update(app: &AppHandle, s: &AutoStatusView) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let (rgba, w, h) = tray_icon_rgba(tray_state(s));
    let _ = tray.set_icon(Some(Image::new_owned(rgba, w, h)));
    let _ = tray.set_tooltip(Some(tray_tooltip(s)));
    if let Ok(m) = menu(app, s) {
        let _ = tray.set_menu(Some(m));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> AutoStatusView {
        AutoStatusView::default()
    }

    #[test]
    fn icone_tem_32x32_rgba() {
        let (v, w, h) = tray_icon_rgba(TrayState::Protegido);
        assert_eq!((w, h), (32, 32));
        assert_eq!(v.len(), 32 * 32 * 4);
    }

    #[test]
    fn estados_tem_cores_diferentes() {
        let px = |s| {
            let (v, w, _) = tray_icon_rgba(s);
            let i = ((DOT_Y * w + DOT_X) * 4) as usize;
            [v[i], v[i + 1], v[i + 2], v[i + 3]]
        };
        let todos = [
            px(TrayState::Protegido),
            px(TrayState::EmRisco),
            px(TrayState::Copiando),
            px(TrayState::Pausado),
        ];
        for a in 0..4 {
            for b in a + 1..4 {
                assert_ne!(todos[a], todos[b], "{a} e {b}");
            }
        }
    }

    #[test]
    fn estado_a_partir_do_status() {
        assert_eq!(tray_state(&st()), TrayState::Protegido);
        assert_eq!(
            tray_state(&AutoStatusView { at_risk: 2, ..st() }),
            TrayState::EmRisco
        );
        assert_eq!(
            tray_state(&AutoStatusView {
                copying: true,
                at_risk: 2,
                ..st()
            }),
            TrayState::Copiando
        );
        assert_eq!(
            tray_state(&AutoStatusView {
                paused: true,
                copying: true,
                ..st()
            }),
            TrayState::Pausado
        );
    }

    #[test]
    fn tooltip_por_estado() {
        assert!(
            tray_tooltip(&AutoStatusView {
                paused: true,
                ..st()
            })
            .contains("pausado")
        );
        assert!(tray_tooltip(&AutoStatusView { at_risk: 2, ..st() }).contains("2 jogos sem cópia"));
        assert!(tray_tooltip(&AutoStatusView { at_risk: 1, ..st() }).contains("1 jogo sem cópia"));
        assert!(tray_tooltip(&st()).contains("protegido"));
        assert!(
            tray_tooltip(&AutoStatusView {
                copying: true,
                ..st()
            })
            .contains("copiando")
        );
        let jogando = AutoStatusView {
            running: vec!["Hades II".into()],
            ..st()
        };
        assert!(tray_tooltip(&jogando).contains("Hades II"));
    }
}
