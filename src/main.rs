#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    mem,
    ops::Deref,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use fltk::{
    enums::{Align, Color},
    prelude::*,
    *,
};
use vmmap::{Process, ProcessInfo, VirtualMemoryRead, VirtualMemoryWrite, VirtualQuery};

// 25 January 2024 – 12:00:48 UTC 3342275935773654592
// // 玩家体力指针
// const CHAIN1: &str =
// "Palworld-Win64-Shipping.exe[0]+143344112@640@2800@208@752";
// // 坐骑体力指针
// const CHAIN2: &str =
// "Palworld-Win64-Shipping.exe[0]+142964888@0@416@1576@752";

// 27 January 2024 - 12:08:12 UTC 5648640939958426658
// 游戏新版本体力都是同一条指针链，但是在不同状态下指向的地址不一样
const CHAIN1: &str = "Palworld-Win64-Shipping.exe[0]+143097744@416@496@40@752";
const CHAIN2: &str = "Palworld-Win64-Shipping.exe[0]+143097744@416@496@40@752";

#[inline]
pub fn get_pointer_chain_address<P, S>(proc: &P, chain: S) -> Option<usize>
where
    P: VirtualMemoryRead + ProcessInfo,
    S: AsRef<str>,
{
    let mut parts = chain.as_ref().split(['[', ']', '+', '@']).filter(|s| !s.is_empty());
    let name = parts.next()?;
    let index = parts.next()?.parse().ok()?;
    let offset = parts.next_back()?.parse().ok()?;
    let elements = parts.map(|s| s.parse());
    let mut address = find_base_address(proc, name, index)?;
    let mut buf = [0; mem::size_of::<usize>()];
    for element in elements {
        let element = element.ok()?;
        proc.read_exact_at(&mut buf, address.checked_add_signed(element)?)
            .ok()?;
        address = usize::from_le_bytes(buf);
    }
    address.checked_add_signed(offset)
}

#[inline]
fn find_base_address<P: ProcessInfo>(proc: &P, name: &str, index: usize) -> Option<usize> {
    proc.get_maps()
        .filter(|m| m.is_read())
        .filter(|m| {
            m.name()
                .and_then(|s| Path::new(s).file_name())
                .is_some_and(|n| n.eq(name))
        })
        .nth(index)
        .map(|x| x.start())
}

pub struct Freeze<P> {
    handle: Option<thread::JoinHandle<()>>,
    ab: Arc<AtomicBool>,
    proc: Arc<P>,
}

impl<P> Freeze<P>
where
    P: VirtualMemoryWrite + VirtualMemoryRead + ProcessInfo + Send + Sync + 'static,
{
    pub fn new(proc: Arc<P>) -> Self {
        Self { handle: None, ab: Arc::new(AtomicBool::new(false)), proc }
    }

    pub fn set_address_with_chain(&self, chain: &str) -> Option<usize> {
        get_pointer_chain_address(self.proc.deref(), chain)
    }

    pub fn freeze(&mut self, addr: usize) -> Result<(), vmmap::Error> {
        self.ab.store(true, Ordering::Relaxed);
        let mut buf = [0u8; mem::size_of::<usize>()];
        self.proc.read_exact_at(&mut buf, addr)?;
        let ab = self.ab.clone();
        let proc = self.proc.clone();
        self.handle = Some(thread::spawn(move || {
            while ab.load(Ordering::Relaxed) {
                if proc.write_all_at(&buf, addr).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(200));
            }
        }));
        Ok(())
    }

    pub fn unfreeze(&mut self) {
        self.ab.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn main() {
    let app = app::App::default().with_scheme(app::Scheme::Base).load_system_fonts();
    let mut win = window::Window::default().with_size(240, 300).center_screen();

    let txt1 = frame::Frame::default()
        .with_size(120, 25)
        .with_label("玩家无限体力: ")
        .with_pos(4, 10);
    let mut btn1 = button::ToggleButton::default()
        .with_size(60, 20)
        .with_align(Align::Inside | Align::Left)
        .with_label("@+9circle")
        .right_of(&txt1, 30);
    btn1.set_frame(enums::FrameType::RFlatBox);
    btn1.set_label_color(Color::White);
    btn1.set_color(Color::from_u32(0x878787));
    btn1.set_selection_color(Color::from_u32(0x147efb));
    btn1.clear_visible_focus();

    let txt2 = frame::Frame::default()
        .with_size(120, 25)
        .with_label("坐骑无限体力: ")
        .below_of(&txt1, 10);
    let mut btn2 = button::ToggleButton::default()
        .with_size(60, 20)
        .with_align(Align::Inside | Align::Left)
        .with_label("@+9circle")
        .right_of(&txt2, 30);
    btn2.set_frame(enums::FrameType::RFlatBox);
    btn2.set_label_color(Color::White);
    btn2.set_color(Color::from_u32(0x878787));
    btn2.set_selection_color(Color::from_u32(0x147efb));
    btn2.clear_visible_focus();

    let mut buf = text::TextBuffer::default();
    buf.set_text(
        "游戏版本: 27 January 2024 - 12:08:12 UTC 5648640939958426658 \
         v0.1.3.0\n\n联机模式也可以使用无限体力；建议每次只启动其中一个开关；如果重新进入游戏、\
         骑乘或结束骑乘需要重新开关对应功能。",
    );
    let mut txt = text::TextDisplay::default()
        .with_size(txt2.width() + btn2.width() + 50, win.height() - txt1.height() - txt2.height() - 30)
        .below_of(&txt2, 10);
    txt.set_buffer(buf);
    txt.wrap_mode(text::WrapMode::AtBounds, 0);

    win.end();
    win.show();

    let mut system = sysinfo::System::new();
    system.refresh_all();

    let pid = match system
        .processes_by_name("Palworld-Win64-Shipping")
        .next()
        .map(|p| p.pid().as_u32())
    {
        Some(id) => id,
        None => {
            let (x, y) = (win.x(), win.y());
            dialog::message(x, y, "请先启动游戏");
            return;
        }
    };

    let proc = match Process::open(pid as _) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            let (x, y) = (win.x(), win.y());
            dialog::message(x, y, &e.to_string());
            return;
        }
    };

    let mut a1 = Freeze::new(proc.clone());
    let mut a2 = Freeze::new(proc);
    btn1.set_callback(move |btn| {
        if btn.is_set() {
            let _ = a1.set_address_with_chain(CHAIN1).map(|a| a1.freeze(a).ok());
            btn.set_align(Align::Inside | Align::Right);
        } else {
            a1.unfreeze();
            btn.set_align(Align::Inside | Align::Left)
        }
        app.redraw();
    });

    btn2.set_callback(move |btn| {
        if btn.is_set() {
            let _ = a2.set_address_with_chain(CHAIN2).map(|a| a2.freeze(a).ok());
            btn.set_align(Align::Inside | Align::Right)
        } else {
            a2.unfreeze();
            btn.set_align(Align::Inside | Align::Left)
        }
        app.redraw();
    });

    app.run().unwrap()
}
