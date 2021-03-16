use x86_64::{structures::paging::PageTable, VirtAddr, PhysAddr};
use x86_64::structures::paging::page_table::PageTableEntry;
use x86_64::structures::paging::*;
use x86_64::structures::paging::page_table::FrameError;
use x86_64::registers::control::Cr3;
use bootloader::BootInfo;
use bootloader::bootinfo::MemoryRegionType;
use crate::println;
use crate::serial_println;


pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable
{
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr
}

pub unsafe fn show_each_table_used_entries(boot_info: &'static BootInfo){
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let l4_table = unsafe { active_level_4_table(phys_mem_offset) };
    for (i, entry) in l4_table.iter().enumerate() {
        show_entry("L4", i, entry);
        let l3_table = get_page_table(entry, boot_info);

        for (i, entry) in l3_table.iter().enumerate() {
            show_entry("L3", i, entry);
            let l2_table = get_page_table(entry, boot_info);

            for (i, entry) in l2_table.iter().enumerate() {
                show_entry("L2", i, entry);
                let l1_table = get_page_table(entry, boot_info);

                for (i, entry) in l1_table.iter().enumerate() {
                    show_entry("L1", i, entry);
                }
            }
        }
    }
}

unsafe fn get_page_table<'a>(entry: &'a PageTableEntry, boot_info: &'static BootInfo) -> &'a PageTable {
    let phys = entry.frame().unwrap().start_address();
    let virt = phys.as_u64() + boot_info.physical_memory_offset;
    let ptr = VirtAddr::new(virt).as_mut_ptr();
    unsafe { &*ptr }
}

unsafe fn show_entry(level: &str, i: usize, entry: &PageTableEntry ) {
    if !entry.is_unused() {
        println!("{} Entry {}: {:?}", level, i, entry);
    }
}

pub unsafe fn translate_addr(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr>
{
    translate_addr_inner(addr, physical_memory_offset)
}

fn translate_addr_inner(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr>
{
    // read the active level 4 frame from the CR3 register
    let (level_4_table_frame, _) = Cr3::read();

    let table_indexes = [
        addr.p4_index(), addr.p3_index(), addr.p2_index(), addr.p1_index()
    ];
    let mut frame = level_4_table_frame;

    // traverse the multi-level page table
    for &index in &table_indexes {
        // convert the frame into a page table reference
        let virt = physical_memory_offset + frame.start_address().as_u64();
        let table_ptr: *const PageTable = virt.as_ptr();
        let table = unsafe {&*table_ptr};

        // read the page table entry and update `frame`
        let entry = &table[index];
        frame = match entry.frame() {
            Ok(frame) => frame,
            Err(FrameError::FrameNotPresent) => return None,
            Err(FrameError::HugeFrame) => panic!("huge pages not supported"),
        };
    }

    // calculate the physical address by adding the page offset
    Some(frame.start_address() + u64::from(addr.page_offset()))
}


unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}

use bootloader::bootinfo::MemoryMap;

/// A FrameAllocator that returns usable frames from the bootloader's memory map.
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize,
}

impl BootInfoFrameAllocator {

    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    /// Returns an iterator over the usable frames specified in the memory map.
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // get usable regions from memory map
        let regions = self.memory_map.iter();
        let usable_regions = regions
            .filter(|r| r.region_type == MemoryRegionType::Usable);
        // map each region to its address range
        let addr_ranges = usable_regions
            .map(|r| r.range.start_addr()..r.range.end_addr());
        // transform to an iterator of frame start addresses
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        // create `PhysFrame` types from the start addresses
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

const VIRTUAL_OFFSET: u64 = 0xC0000000;

pub unsafe fn to_virt(phys_addr: &PhysAddr) -> Option<VirtAddr> {
    if phys_addr.as_u64() < 0x100000000 {
        Some(VirtAddr::new(phys_addr.as_u64() + VIRTUAL_OFFSET))
    } else {
        None
    }
}