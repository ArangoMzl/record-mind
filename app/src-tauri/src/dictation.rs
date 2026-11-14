//! Dictado por voz: abre el dictado de Windows (Win+H) sobre el campo que tiene el foco.
//! Windows ya transcribe en cualquier campo de texto; la app solo pulsa el atajo por ti.

/// Pulsa Win+H como si fuera el teclado. La interfaz pone antes el foco en la nota.
#[cfg(windows)]
pub fn open() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_LWIN,
    };

    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let h = VIRTUAL_KEY(b'H' as u16);
    let inputs = [key(VK_LWIN, false), key(h, false), key(h, true), key(VK_LWIN, true)];
    // SAFETY: el arreglo vive durante la llamada y cada INPUT está completo.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err("Windows no dejó abrir el dictado. Pulsa Win+H a mano.".into())
    }
}

#[cfg(not(windows))]
pub fn open() -> Result<(), String> {
    Err("El dictado por voz usa el de Windows (Win+H)".into())
}
