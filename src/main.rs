#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(const_mut_refs)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(asm)]

extern crate alloc;

mod vga;
mod allocator;
mod serial;
mod gdt;
mod interrupts;
mod memory;
mod calculator;

use crate::allocator::alloc::Locked;
use crate::allocator::list::Allocator;
use crate::memory::memory_management::BootInfoFrameAllocator;

use bootloader::BootInfo;
use bootloader::entry_point;
use x86_64::instructions::interrupts::int3;
use x86_64::instructions::port::Port;
use x86_64::structures::paging::{PageTable, Page, Translate};
use alloc::{boxed::Box, vec, vec::Vec, rc::Rc};
use core::panic::PanicInfo;
use crate::calculator::display::*;
use crate::allocator::bump_allocator::BumpAllocator;

#[global_allocator]
static ALLOCATOR: Locked<Allocator> = Locked::new(Allocator::new());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    use x86_64::VirtAddr;
    use x86_64::registers::control::Cr3;

    display_menu();

    println!("Hello fucking World{}", "!");
    interrupts::init_idt();
    gdt::gdt_init();
    unsafe {
        interrupts::PICS.lock().initialize();
    }
    x86_64::instructions::interrupts::enable();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::memory_management::init(phys_mem_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::alloc::init_heap(&mut mapper, &mut frame_allocator, &boot_info).expect("heap initialization failed");

    let heap_value = Box::new(41);
    println!("heap_value at {:p}", heap_value);

    let mut vec = Vec::new();
    for i in 0..500 {
        vec.push(i);
    }
    println!("vec at {:p}", vec.as_slice());
    println!("Press any key to reload screen...");

    #[cfg(test)]
        test_main();

    loop {
        x86_64::instructions::hlt();
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", _info);
    exit_qemu(QemuExitCode::Failed);
    loop { }
}

#[cfg(test)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", _info);
    loop{}
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    println!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
    exit_qemu(QemuExitCode::Success);
}

#[test_case]
fn first_test() {
    let x = 1;
    let y = 1;
    assert_eq!(x, y);
    serial_println!("x: {} == y: {}", x, y);
    serial_println!("[ok]");
}

#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}
