// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright (c) 2018-2021 Andre Richter <andre.o.richter@gmail.com>

//! BSP Memory Management Unit.

use crate::{
    common,
    memory::{
        mmu as generic_mmu,
        mmu::{
            AddressSpace, AssociatedTranslationTable, AttributeFields, Page, PageSliceDescriptor,
            TranslationGranule,
        },
        Address, Physical, Virtual,
    },
    synchronization::InitStateLock,
};

//--------------------------------------------------------------------------------------------------
// Private Definitions
//--------------------------------------------------------------------------------------------------

type KernelTranslationTable =
    <KernelVirtAddrSpace as AssociatedTranslationTable>::TableStartFromTop;

//--------------------------------------------------------------------------------------------------
// Public Definitions
//--------------------------------------------------------------------------------------------------

/// The translation granule chosen by this BSP. This will be used everywhere else in the kernel to
/// derive respective data structures and their sizes. For example, the `crate::memory::mmu::Page`.
pub type KernelGranule = TranslationGranule<{ 64 * 1024 }>;

/// The kernel's virtual address space defined by this BSP.
pub type KernelVirtAddrSpace = AddressSpace<{ get_virt_addr_space_size() }>;

//--------------------------------------------------------------------------------------------------
// Global instances
//--------------------------------------------------------------------------------------------------

/// The kernel translation tables.
///
/// It is mandatory that InitStateLock is transparent.
///
/// That is, `size_of(InitStateLock<KernelTranslationTable>) == size_of(KernelTranslationTable)`.
/// There is a unit tests that checks this porperty.
#[link_section = ".data"]
static KERNEL_TABLES: InitStateLock<KernelTranslationTable> =
    InitStateLock::new(KernelTranslationTable::new_for_precompute());

/// This value is needed during early boot for MMU setup.
///
/// This will be patched to the correct value by the "translation table tool" after linking. This
/// given value here is just a dummy.
#[link_section = ".text._start_arguments"]
#[no_mangle]
static PHYS_KERNEL_TABLES_BASE_ADDR: u64 = 0xCCCCAAAAFFFFEEEE;

//--------------------------------------------------------------------------------------------------
// Private Code
//--------------------------------------------------------------------------------------------------

/// This is a hack for retrieving the value for the kernel's virtual address space size as a
/// constant from a common place, since it is needed as a compile-time/link-time constant in both,
/// the linker script and the Rust sources.
const fn get_virt_addr_space_size() -> usize {
    let __kernel_virt_addr_space_size;

    include!("../kernel_virt_addr_space_size.ld");

    __kernel_virt_addr_space_size
}

/// Helper function for calculating the number of pages the given parameter spans.
const fn size_to_num_pages(size: usize) -> usize {
    assert!(size > 0);
    assert!(size % KernelGranule::SIZE == 0);

    size >> KernelGranule::SHIFT
}

/// The Read+Execute (RX) pages of the kernel binary.
fn virt_rx_page_desc() -> PageSliceDescriptor<Virtual> {
    let num_pages = size_to_num_pages(super::rx_size());

    PageSliceDescriptor::from_addr(super::virt_rx_start(), num_pages)
}

/// The Read+Write (RW) pages of the kernel binary.
fn virt_rw_page_desc() -> PageSliceDescriptor<Virtual> {
    let num_pages = size_to_num_pages(super::rw_size());

    PageSliceDescriptor::from_addr(super::virt_rw_start(), num_pages)
}

/// The boot core's stack.
fn virt_boot_core_stack_page_desc() -> PageSliceDescriptor<Virtual> {
    let num_pages = size_to_num_pages(super::boot_core_stack_size());

    PageSliceDescriptor::from_addr(super::virt_boot_core_stack_start(), num_pages)
}

// There is no reason to expect the following conversions to fail, since they were generated offline
// by the `translation table tool`. If it doesn't work, a panic due to the unwraps is justified.
fn kernel_virt_to_phys_page_slice(
    virt_slice: PageSliceDescriptor<Virtual>,
) -> PageSliceDescriptor<Physical> {
    let phys_first_page =
        generic_mmu::try_kernel_virt_page_ptr_to_phys_page_ptr(virt_slice.first_page_ptr())
            .unwrap();
    let phys_start_addr = Address::new(phys_first_page as usize);

    PageSliceDescriptor::from_addr(phys_start_addr, virt_slice.num_pages())
}

fn kernel_page_attributes(virt_page_ptr: *const Page<Virtual>) -> AttributeFields {
    generic_mmu::try_kernel_page_attributes(virt_page_ptr).unwrap()
}

//--------------------------------------------------------------------------------------------------
// Public Code
//--------------------------------------------------------------------------------------------------

/// Return a reference to the kernel's translation tables.
pub fn kernel_translation_tables() -> &'static InitStateLock<KernelTranslationTable> {
    &KERNEL_TABLES
}

/// Pointer to the last page of the physical address space.
pub fn phys_addr_space_end_page_ptr() -> *const Page<Physical> {
    common::align_down(
        super::phys_addr_space_end().into_usize(),
        KernelGranule::SIZE,
    ) as *const Page<_>
}

/// Add mapping records for the kernel binary.
///
/// The actual translation table entries for the kernel binary are generated using the offline
/// `translation table tool` and patched into the kernel binary. This function just adds the mapping
/// record entries.
pub fn kernel_add_mapping_records_for_precomputed() {
    let virt_rx_page_desc = virt_rx_page_desc();
    generic_mmu::kernel_add_mapping_record(
        "Kernel code and RO data",
        &virt_rx_page_desc,
        &kernel_virt_to_phys_page_slice(virt_rx_page_desc),
        &kernel_page_attributes(virt_rx_page_desc.first_page_ptr()),
    );

    let virt_rw_page_desc = virt_rw_page_desc();
    generic_mmu::kernel_add_mapping_record(
        "Kernel data and bss",
        &virt_rw_page_desc,
        &kernel_virt_to_phys_page_slice(virt_rw_page_desc),
        &kernel_page_attributes(virt_rw_page_desc.first_page_ptr()),
    );

    let virt_boot_core_stack_page_desc = virt_boot_core_stack_page_desc();
    generic_mmu::kernel_add_mapping_record(
        "Kernel boot-core stack",
        &virt_boot_core_stack_page_desc,
        &kernel_virt_to_phys_page_slice(virt_boot_core_stack_page_desc),
        &kernel_page_attributes(virt_boot_core_stack_page_desc.first_page_ptr()),
    );
}
