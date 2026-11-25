//! agentspace's per-agent footprint, and the size a number of agents at once gets from it.
//!
//! The footprint is D73's, kept here as its one named place, outside the contract (D76): a new
//! figure changes which size a plan gets, never what a field means. The sizes and their floors are
//! the contract's (docs/lease-api.md, Size; D65). The sizes each footprint gives are computed from
//! the two, never written out.

/// The vCPUs each agent at once is given, in thousandths of a vCPU: half a vCPU (D73).
pub const PER_AGENT_MILLIVCPUS: u32 = 500;

/// The memory each agent at once is given, in MiB: 2 GiB (D73).
pub const PER_AGENT_MIB: u32 = 2048;

/// A size of lease-api.md's table: its name, its floor of vCPUs and its floor of memory in GiB,
/// smallest first.
const SIZES: [(&str, u32, u32); 4] = [("s", 2, 8), ("m", 4, 16), ("l", 8, 32), ("xl", 16, 64)];

/// How many agents at once a size's floors hold at the footprint: the fewer of what its vCPUs hold
/// and what its memory holds.
fn holds(vcpus: u32, memory_gib: u32) -> u32 {
    let by_cpu = (u64::from(vcpus) * 1000)
        .checked_div(u64::from(PER_AGENT_MILLIVCPUS))
        .unwrap_or(u64::MAX);
    let by_memory = (u64::from(memory_gib) * 1024)
        .checked_div(u64::from(PER_AGENT_MIB))
        .unwrap_or(u64::MAX);
    u32::try_from(by_cpu.min(by_memory)).unwrap_or(u32::MAX)
}

/// The smallest of `s`, `m`, `l` and `xl` whose vCPUs and memory are both at least `agents` times
/// the per-agent footprint, or `None` when no size holds them (docs/lease-api.md, Plan; D75).
pub fn size_for(agents: u32) -> Option<&'static str> {
    SIZES
        .iter()
        .find(|(_, vcpus, memory_gib)| agents <= holds(*vcpus, *memory_gib))
        .map(|(name, _, _)| *name)
}

/// The most agents at once the largest size holds at the footprint: 32 at D73's figure. A plan
/// whose busiest turn has more is answered `plan_too_large`.
pub fn most_agents() -> u32 {
    SIZES
        .iter()
        .map(|(_, vcpus, memory_gib)| holds(*vcpus, *memory_gib))
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// lease-api.md's table of what each size holds at today's footprint (Plan; pin P11).
    fn expected(agents: u32) -> Option<&'static str> {
        match agents {
            0..=4 => Some("s"),
            5..=8 => Some("m"),
            9..=16 => Some("l"),
            17..=32 => Some("xl"),
            _ => None,
        }
    }

    #[test]
    fn each_count_gets_the_size_lease_api_gives_at_d73s_figure() {
        for agents in 0..=200 {
            assert_eq!(size_for(agents), expected(agents), "agents {agents}");
        }
    }

    #[test]
    fn the_largest_size_holds_32_agents_at_d73s_figure() {
        assert_eq!(most_agents(), 32);
        assert_eq!(size_for(most_agents()), Some("xl"));
        assert_eq!(size_for(most_agents() + 1), None);
    }

    #[test]
    fn the_largest_counts_neither_overflow_nor_find_a_size() {
        assert_eq!(size_for(u32::MAX), None);
        assert_eq!(size_for(u32::MAX - 1), None);
    }

    #[test]
    fn a_size_holds_the_fewer_of_what_its_cpu_and_its_memory_hold() {
        // 2 vCPUs hold 4 agents at half a vCPU each; 4 GiB holds 2 at 2 GiB each.
        assert_eq!(holds(2, 4), 2);
        // 1 vCPU holds 2; 64 GiB holds 32.
        assert_eq!(holds(1, 64), 2);
        assert_eq!(holds(16, 64), 32);
    }
}
