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

use crate::allocator::alloc::{Locked, HEAP_SIZE};
use crate::allocator::list::Allocator;
use crate::memory::memory_management::BootInfoFrameAllocator;

use bootloader::BootInfo;
use bootloader::entry_point;
use x86_64::instructions::interrupts::int3;
use x86_64::instructions::port::Port;
use x86_64::structures::paging::{PageTable, Page, Translate};
use alloc::{boxed::Box, vec, vec::Vec, rc::Rc};
use core::panic::PanicInfo;
use crate::allocator::bump_allocator::BumpAllocator;
use crate::allocator::buddy_system::buddy_manager::{LockedHeap, Heap};
use crate::allocator::buddy_system::linked_list;
use crate::allocator::buddy_system::frame::FrameAllocator;
use core::alloc::Layout;
use core::ptr::NonNull;
use core::mem::size_of;

#[global_allocator]
static BUDDY_ALLOCATOR: LockedHeap = LockedHeap::empty();

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

    interrupts::init_idt();
    gdt::gdt_init();
    unsafe {
        interrupts::PICS.lock().initialize();
    }
    x86_64::instructions::interrupts::enable();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::memory_management::init(phys_mem_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::alloc::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

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

#[test_case]
fn test_empty_heap() {
    serial_println!("[Test]: test_empty_heap");
    let mut heap = Heap::new();
    assert!(heap.alloc(Layout::from_size_align(1, 1).unwrap()).is_err());
    serial_println!("[ok]");
    serial_println!();
}

#[test_case]
fn test_heap_add() {
    serial_println!("[Test]: heap_add");
    let mut heap = Heap::new();
    assert!(heap.alloc(Layout::from_size_align(1, 1).unwrap()).is_err());

    let space: [usize; 100] = [0; 100];
    unsafe {
        heap.add_to_heap(space.as_ptr() as usize, space.as_ptr().add(100) as usize);
    }
    let addr = heap.alloc(Layout::from_size_align(1, 1).unwrap());
    assert!(addr.is_ok());
    serial_println!("[ok]");
    serial_println!();
}

#[test_case]
fn test_heap_oom() {
    serial_println!("[Test]: heap_oom");
    let mut heap = Heap::new();
    let space: [usize; 100] = [0; 100];
    unsafe {
        heap.add_to_heap(space.as_ptr() as usize, space.as_ptr().add(100) as usize);
    }

    assert!(heap
        .alloc(Layout::from_size_align(100 * size_of::<usize>(), 1).unwrap())
        .is_err());
    assert!(heap.alloc(Layout::from_size_align(1, 1).unwrap()).is_ok());
    serial_println!("[ok]");
    serial_println!();
}


#[test_case]
fn test_heap_alloc_and_free() {
    serial_println!("[Test]: heap_alloc_and_free");
    let mut heap = Heap::new();
    assert!(heap.alloc(Layout::from_size_align(1, 1).unwrap()).is_err());

    let space: [usize; 100] = [0; 100];
    unsafe {
        heap.add_to_heap(space.as_ptr() as usize, space.as_ptr().add(100) as usize);
    }
    for _ in 0..100 {
        let addr = heap.alloc(Layout::from_size_align(1, 1).unwrap()).unwrap();
        heap.dealloc(addr, Layout::from_size_align(1, 1).unwrap());
    }
    serial_println!("[ok]");
    serial_println!();
}

#[test_case]
fn test_empty_frame_allocator() {
    serial_println!("[Test]: empty_frame_allocator");
    let mut frame = FrameAllocator::new();
    assert!(frame.alloc(1).is_none());
    serial_println!("[ok]");
    serial_println!();
}

#[test_case]
fn test_frame_allocator_add() {
    serial_println!("[Test]: frame_allocator_add");
    let mut frame = FrameAllocator::new();
    assert!(frame.alloc(1).is_none());

    frame.insert(0..3);
    let num = frame.alloc(1);
    assert_eq!(num, Some(2));
    let num = frame.alloc(2);
    assert_eq!(num, Some(0));
    assert!(frame.alloc(1).is_none());
    assert!(frame.alloc(2).is_none());
    serial_println!("[ok]");
    serial_println!();
}

#[test_case]
fn test_frame_allocator_alloc_and_free() {
    serial_println!("[Test]: frame_allocator_alloc_and_free");
    let mut frame = FrameAllocator::new();
    assert!(frame.alloc(1).is_none());

    frame.add_frame(0, 1024);
    for _ in 0..100 {
        let addr = frame.alloc(512).unwrap();
        frame.dealloc(addr, 512);
    }
    serial_println!("[ok]");
    serial_println!();
}

#[test_case]
fn test_frame_allocator_alloc_and_free_complex() {
    serial_println!("[Test]: frame_allocator_alloc_and_free_complex");
    let mut frame = FrameAllocator::new();
    frame.add_frame(100, 1024);
    for _ in 0..10 {
        let addr = frame.alloc(1).unwrap();
        frame.dealloc(addr, 1);
    }
    let addr1 = frame.alloc(1).unwrap();
    let addr2 = frame.alloc(1).unwrap();
    assert_ne!(addr1, addr2);
    serial_println!("[ok]")
}

#[test_case]
fn simple_allocation() {
    serial_println!("[Test]: simple_allocation");
    let heap_value_1 = Box::new(41);
    let heap_value_2 = Box::new(13);
    assert_eq!(*heap_value_1, 41);
    assert_eq!(*heap_value_2, 13);
    serial_println!("{:p}", heap_value_1);
    serial_println!("{:p}", heap_value_2);
    BUDDY_ALLOCATOR.show();
    serial_println!("[ok]");
    serial_println!();
}

#[test_case]
fn small_vec() {
    serial_println!("[Test]: small_vec");
    let n = 10;
    let mut vec = Vec::new();
    for i in 0..n {
        vec.push(i);
    }
    assert_eq!(vec.iter().sum::<u64>(), (n - 1) * n / 2);
    serial_println!("{:p}", vec.as_slice());
    unsafe {
        let x = BUDDY_ALLOCATOR.lock().alloc(
            Layout::from_size_align_unchecked(core::mem::size_of::<i32>() * 4, 1)).expect("allocation failed");
        let mut x = x.as_ptr();
        x.write(2);
        x.add(3).write(10);
        
        serial_println!("{:?}", x.as_ref());
        serial_println!("{:?}", x.offset(3).as_ref());

        assert_eq!(x.as_ref(), 2);
        assert_eq!(x.offset(3).as_ref(), 10);
        let x = NonNull::new(x).expect("error");
        BUDDY_ALLOCATOR.lock().dealloc(x, Layout::for_value(&x));
        serial_println!("{:?}", x.as_ref());
    }
    BUDDY_ALLOCATOR.show();
    serial_println!("[ok]");
    serial_println!();
}

#[test_case]
fn large_vec() {
    serial_println!("[Test]: large_vec");
    let n = 1000;
    let mut vec = Vec::new();
    for i in 0..n {
        vec.push(i);
    }
    assert_eq!(vec.iter().sum::<u64>(), (n - 1) * n / 2);
    serial_println!("{:p}", vec.as_slice());
    BUDDY_ALLOCATOR.show();
    serial_println!("[ok]");
    serial_println!();
}

#[test_case]
fn many_boxes() {
    serial_println!("[Test]: many_boxes");
    for i in 0..HEAP_SIZE {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
    BUDDY_ALLOCATOR.show();
    serial_println!("[ok]");
    serial_println!();
}


#[test_case]
fn many_boxes_long_lived() {
    serial_println!("[Test]: many_boxes_long_lived");
    let long_lived = Box::new(1);
    for i in 0..HEAP_SIZE {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
    assert_eq!(*long_lived, 1);
    BUDDY_ALLOCATOR.show();
    serial_println!("[ok]");
    serial_println!();
}

#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}
