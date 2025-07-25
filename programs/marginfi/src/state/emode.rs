use crate::errors::MarginfiError;
use crate::prelude::MarginfiResult;
use crate::state::marginfi_group::WrappedI80F48;
use crate::{assert_struct_align, assert_struct_size, check};
use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
use fixed::types::I80F48;
use type_layout::TypeLayout;

// Enable eMode flag
pub const EMODE_ON: u64 = 1;

// Limit each config to 10 entries
pub const MAX_EMODE_ENTRIES: usize = 10;
// Represents an invalid tag, used as a sentinel value
pub const EMODE_TAG_EMPTY: u16 = 0;

assert_struct_size!(EmodeSettings, 424);
assert_struct_align!(EmodeSettings, 8);
#[repr(C)]
#[derive(
    AnchorSerialize, AnchorDeserialize, Debug, PartialEq, Eq, Pod, Zeroable, Copy, Clone, TypeLayout,
)]
pub struct EmodeSettings {
    // The eMode type of the current bank (for example, all stablecoins may be tag=1)
    pub emode_tag: u16,
    pub pad0: [u8; 6],
    pub timestamp: i64,
    pub flags: u64,
    // A collection of eMode policies defined for this bank (maximum 10 entries)
    pub emode_config: EmodeConfig,
}

// Returns an all-zero structure to facilitate initialization of on-chain accounts
impl Default for EmodeSettings {
    fn default() -> Self {
        Self::zeroed()
    }
}

impl EmodeSettings {
    pub fn validate_entries(&self) -> MarginfiResult {
        for entry in self.emode_config.entries {
            if entry.is_empty() {
                continue;
            }
            let asset_init_w: I80F48 = I80F48::from(entry.asset_weight_init);
            let asset_maint_w: I80F48 = I80F48::from(entry.asset_weight_maint);

            // The initial mortgage rate must be between 0 (equivalent to 0%) and 1 (equivalent to 100%)
            check!(
                asset_init_w >= I80F48::ZERO && asset_init_w <= I80F48::ONE,
                MarginfiError::BadEmodeConfig
            );
            // The liquidation mortgage ratio cannot be higher than 2 (200%) - a reasonable upper limit is reserved
            check!(
                asset_maint_w <= (I80F48::ONE + I80F48::ONE),
                MarginfiError::InvalidConfig
            );
            // The maintenance mortgage rate must be ≥ the initial mortgage rate (otherwise the user will be liquidated as soon as the loan is completed)
            check!(asset_maint_w >= asset_init_w, MarginfiError::BadEmodeConfig);
        }

        // Check if there are duplicate tags in all entries
        self.check_dupes()?;

        Ok(())
    }

    fn check_dupes(&self) -> MarginfiResult {
        let non_empty_tags: Vec<u16> = self
            .emode_config
            .entries
            .iter()
            .filter(|e| !e.is_empty())
            .map(|e| e.collateral_bank_emode_tag)
            .collect();

        if non_empty_tags.windows(2).any(|w| w[0] == w[1]) {
            err!(MarginfiError::BadEmodeConfig)
        } else {
            Ok(())
        }
    }

    // Check whether the EMODE_ON flag is set, that is, whether the current emode is enabled
    pub fn is_enabled(&self) -> bool {
        self.flags & EMODE_ON != 0
    }

    // Enable/disable emode function
    pub fn set_emode_enabled(&mut self, enabled: bool) {
        if enabled {
            self.flags |= EMODE_ON;
        } else {
            self.flags &= !EMODE_ON;
        }
    }
}

assert_struct_size!(EmodeConfig, 400);
assert_struct_align!(EmodeConfig, 8);
#[repr(C)]
#[derive(
    AnchorDeserialize, AnchorSerialize, Debug, PartialEq, Eq, Pod, Zeroable, Copy, Clone, TypeLayout,
)]
pub struct EmodeConfig {
    pub entries: [EmodeEntry; MAX_EMODE_ENTRIES],
}

impl EmodeConfig {
    pub fn from_entries(entries: &[EmodeEntry]) -> Self {
        let count = entries.len();
        if count > MAX_EMODE_ENTRIES {
            panic!(
                "Too many EmodeEntry items {:?}, maximum allowed {:?}",
                count, MAX_EMODE_ENTRIES
            );
        }

        let mut config = Self::zeroed();
        for (i, entry) in entries.iter().enumerate() {
            config.entries[i] = *entry;
        }
        config.entries[..count].sort_by_key(|e| e.collateral_bank_emode_tag);

        config
    }

    pub fn find_with_tag(&self, tag: u16) -> Option<&EmodeEntry> {
        if tag == EMODE_TAG_EMPTY {
            return None;
        }
        self.entries.iter().find(|e| e.tag_equals(tag))
    }

    pub fn has_entries(&self) -> bool {
        self.entries.iter().any(|e| !e.is_empty())
    }
}

assert_struct_size!(EmodeEntry, 40);
assert_struct_align!(EmodeEntry, 8);
#[repr(C)]
#[derive(
    AnchorDeserialize, AnchorSerialize, Debug, PartialEq, Eq, Pod, Zeroable, Copy, Clone, TypeLayout,
)]
pub struct EmodeEntry {
    // Which type of collateral object is applicable to this strategy (e.g. tag=1 is a stablecoin)
    pub collateral_bank_emode_tag: u16,
    pub flags: u8,
    pub pad0: [u8; 5],
    // Initial asset weight for lending (affects the maximum loan amount)
    pub asset_weight_init: WrappedI80F48,
    // Liquidation asset weight (affects when liquidation occurs)
    pub asset_weight_maint: WrappedI80F48,
}

impl EmodeEntry {
    pub fn is_empty(&self) -> bool {
        self.collateral_bank_emode_tag == EMODE_TAG_EMPTY
    }
    pub fn tag_equals(&self, tag: u16) -> bool {
        self.collateral_bank_emode_tag == tag
    }
}
