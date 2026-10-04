//! An explicit user selection of any nonempty set of peripherals.
use crate::runtime_recovery::Role;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Part(Role),
    Pair(Role, Role),
    Whole,
}
impl Scope {
    pub fn from_roles(roles: impl IntoIterator<Item = Role>) -> Option<Self> {
        let roles: Vec<_> = roles.into_iter().collect();
        let canonical: Vec<_> = [Role::Left, Role::Right, Role::Receiver]
            .into_iter()
            .filter(|role| roles.contains(role))
            .collect();
        match canonical.as_slice() {
            [] => None,
            [role] => Some(Self::Part(*role)),
            [first, second] => Some(Self::Pair(*first, *second)),
            _ => Some(Self::Whole),
        }
    }
    pub fn contains(self, role: Role) -> bool {
        self.roles().contains(&role)
    }
    pub fn single(self) -> Option<Role> {
        let roles = self.roles();
        (roles.len() == 1).then(|| roles[0])
    }
    pub fn roles(self) -> Vec<Role> {
        match self {
            Self::Part(role) => vec![role],
            Self::Pair(first, second) => [Role::Left, Role::Right, Role::Receiver]
                .into_iter()
                .filter(|role| *role == first || *role == second)
                .collect(),
            Self::Whole => vec![Role::Left, Role::Right, Role::Receiver],
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Part(Role::Left) => "Left half",
            Self::Part(Role::Right) => "Right half",
            Self::Part(Role::Receiver) => "USB dongle",
            Self::Pair(Role::Left, Role::Right) | Self::Pair(Role::Right, Role::Left) => {
                "Left and right halves"
            }
            Self::Pair(Role::Left, Role::Receiver) | Self::Pair(Role::Receiver, Role::Left) => {
                "Left half and dongle"
            }
            Self::Pair(Role::Right, Role::Receiver) | Self::Pair(Role::Receiver, Role::Right) => {
                "Right half and dongle"
            }
            Self::Pair(Role::Left, Role::Left) => "Left half",
            Self::Pair(Role::Right, Role::Right) => "Right half",
            Self::Pair(Role::Receiver, Role::Receiver) => "USB dongle",
            Self::Whole => "Whole keyboard",
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subsets_are_canonical_and_duplicates_do_not_create_extra_parts() {
        assert_eq!(Scope::from_roles([]), None);
        assert_eq!(
            Scope::from_roles([Role::Receiver, Role::Left, Role::Receiver]),
            Some(Scope::Pair(Role::Left, Role::Receiver))
        );
        for mask in 1..8 {
            let selected = [Role::Left, Role::Right, Role::Receiver]
                .into_iter()
                .enumerate()
                .filter_map(|(index, role)| (mask & (1 << index) != 0).then_some(role))
                .collect::<Vec<_>>();
            let scope = Scope::from_roles(selected.clone()).unwrap();
            assert_eq!(scope.roles(), selected);
            for role in [Role::Left, Role::Right, Role::Receiver] {
                assert_eq!(scope.contains(role), selected.contains(&role));
            }
            assert_eq!(scope.single(), (selected.len() == 1).then(|| selected[0]));
        }
    }
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
