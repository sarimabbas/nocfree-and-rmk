//! An explicit user selection of one peripheral or the complete keyboard.
use crate::runtime_recovery::Role;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Part(Role),
    Whole,
}
impl Scope {
    pub fn roles(self) -> Vec<Role> {
        match self {
            Self::Part(role) => vec![role],
            Self::Whole => vec![Role::Left, Role::Right, Role::Receiver],
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Part(Role::Left) => "Left half",
            Self::Part(Role::Right) => "Right half",
            Self::Part(Role::Receiver) => "USB dongle",
            Self::Whole => "Whole keyboard",
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_has_explicit_stable_part_order() {
        assert_eq!(
            Scope::Whole.roles(),
            vec![Role::Left, Role::Right, Role::Receiver]
        );
        for role in [Role::Left, Role::Right, Role::Receiver] {
            assert_eq!(Scope::Part(role).roles(), vec![role]);
        }
    }
}
