#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod projects {
    use ink::prelude::vec::Vec;
    use ink::prelude::string::String;
    use ink::prelude::collections::BTreeMap;

    #[derive(Debug, scale::Encode, scale::Decode, PartialEq, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct TeamMember {
        account_id: AccountId,
        role: String,
        rating: Option<u8>,
    }

    #[derive(Debug, scale::Encode, scale::Decode, PartialEq)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum ProjectStatus {
        Created,
        CoordinatorAssigned,
        TeamAssigned,
        Completed,
    }

    /// Interface for the calendar contract
    #[ink::trait_definition]
    pub trait Calendar {
        #[ink(message)]
        fn is_available(&self, account_id: AccountId, project_id: AccountId) -> bool;
        
        #[ink(message)]
        fn get_available_workers(&self, project_id: AccountId) -> Vec<AccountId>;
    }

    #[ink(storage)]
    pub struct Project {
        name: String,
        client: AccountId,
        dao_address: AccountId,
        coordinator: Option<AccountId>,
        team_members: Vec<TeamMember>,
        status: ProjectStatus,
        max_name_length: u32,
        calendar_contract: Option<AccountId>,
    }

    #[ink(event)]
    pub struct CoordinatorAssigned {
        #[ink(topic)]
        project: String,
        #[ink(topic)]
        coordinator: AccountId,
    }

    #[ink(event)]
    pub struct TeamAssigned {
        #[ink(topic)]
        project: String,
        team_size: u32,
    }

    #[ink(event)]
    pub struct ProjectCompleted {
        #[ink(topic)]
        project: String,
        #[ink(topic)]
        client: AccountId,
    }

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        NameTooLong,
        NotAuthorized,
        CoordinatorNotAssigned,
        TeamMemberNotFound,
        InvalidRatingValue,
        RatingsCountMismatch,
        ProjectAlreadyCompleted,
        CalendarContractNotSet,
        NoAvailableCoordinators,
        NoAvailableTeamMembers,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    impl Project {
        #[ink(constructor)]
        pub fn new(name: String, dao_address: AccountId, calendar_contract: Option<AccountId>) -> Result<Self> {
            let max_name_length = 50;
            if name.len() > max_name_length as usize {
                return Err(Error::NameTooLong);
            }

            Ok(Self {
                name,
                client: Self::env().caller(),
                dao_address,
                coordinator: None,
                team_members: Vec::new(),
                status: ProjectStatus::Created,
                max_name_length,
                calendar_contract,
            })
        }

        #[ink(message)]
        pub fn assign_coordinator(&mut self) -> Result<AccountId> {
            let caller = self.env().caller();
            if caller != self.dao_address {
                return Err(Error::NotAuthorized);
            }

            let coordinator = self.select_coordinator()?;
            self.coordinator = Some(coordinator);
            self.status = ProjectStatus::CoordinatorAssigned;
            
            self.env().emit_event(CoordinatorAssigned {
                project: self.name.clone(),
                coordinator,
            });

            Ok(coordinator)
        }

        #[ink(message)]
        pub fn assign_team(&mut self) -> Result<Vec<TeamMember>> {
            let caller = self.env().caller();
            
            if let Some(coordinator) = self.coordinator {
                if caller != coordinator {
                    return Err(Error::NotAuthorized);
                }
            } else {
                return Err(Error::CoordinatorNotAssigned);
            }

            let team_members = self.select_team_members()?;
            self.team_members = team_members.clone();
            self.status = ProjectStatus::TeamAssigned;

            self.env().emit_event(TeamAssigned {
                project: self.name.clone(),
                team_size: self.team_members.len() as u32,
            });

            Ok(team_members)
        }

        #[ink(message)]
        pub fn mark_completed(&mut self, ratings: Vec<(AccountId, u8)>) -> Result<()> {
            let caller = self.env().caller();
            
            if caller != self.client {
                return Err(Error::NotAuthorized);
            }

            if self.status == ProjectStatus::Completed {
                return Err(Error::ProjectAlreadyCompleted);
            }

            if ratings.len() != self.team_members.len() {
                return Err(Error::RatingsCountMismatch);
            }

            // Apply ratings to team members
            for (account_id, rating) in ratings {
                if rating > 10 {
                    return Err(Error::InvalidRatingValue);
                }

                let team_member = self.team_members
                    .iter_mut()
                    .find(|m| m.account_id == account_id);

                if let Some(member) = team_member {
                    member.rating = Some(rating);
                } else {
                    return Err(Error::TeamMemberNotFound);
                }
            }

            self.status = ProjectStatus::Completed;

            self.env().emit_event(ProjectCompleted {
                project: self.name.clone(),
                client: self.client,
            });

            Ok(())
        }

        #[ink(message)]
        pub fn get_project_info(&self) -> (String, AccountId, Option<AccountId>, ProjectStatus) {
            (
                self.name.clone(),
                self.client,
                self.coordinator,
                self.status.clone(),
            )
        }

        #[ink(message)]
        pub fn get_team(&self) -> Vec<TeamMember> {
            self.team_members.clone()
        }

        #[ink(message)]
        pub fn set_calendar_contract(&mut self, calendar_contract: AccountId) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.dao_address {
                return Err(Error::NotAuthorized);
            }
            
            self.calendar_contract = Some(calendar_contract);
            Ok(())
        }
    }

    // Private implementation for internal methods
    impl Project {
        fn select_coordinator(&self) -> Result<AccountId> {
            // Get calendar contract
            let calendar_contract = match self.calendar_contract {
                Some(address) => address,
                None => return Err(Error::CalendarContractNotSet),
            };

            // Get available coordinators from calendar contract
            let mut available_workers = ink::env::call::build_call::<ink::env::DefaultEnvironment>()
                .call(calendar_contract)
                .gas_limit(0)
                .transferred_value(0)
                .exec_input(
                    ink::env::call::ExecutionInput::new(ink::env::call::Selector::new(
                        ink::selector_bytes!("get_available_workers")
                    ))
                    .push_arg(self.env().account_id())
                )
                .returns::<Vec<AccountId>>()
                .invoke();

            // Select the first available worker as coordinator
            // In a real implementation, you might have more complex selection logic
            if let Some(coordinator) = available_workers.first() {
                Ok(*coordinator)
            } else {
                Err(Error::NoAvailableCoordinators)
            }
        }

        fn select_team_members(&self) -> Result<Vec<TeamMember>> {
            // Get calendar contract
            let calendar_contract = match self.calendar_contract {
                Some(address) => address,
                None => return Err(Error::CalendarContractNotSet),
            };

            // Get available workers from calendar contract
            let available_workers = ink::env::call::build_call::<ink::env::DefaultEnvironment>()
                .call(calendar_contract)
                .gas_limit(0)
                .transferred_value(0)
                .exec_input(
                    ink::env::call::ExecutionInput::new(ink::env::call::Selector::new(
                        ink::selector_bytes!("get_available_workers")
                    ))
                    .push_arg(self.env().account_id())
                )
                .returns::<Vec<AccountId>>()
                .invoke();

            if available_workers.is_empty() {
                return Err(Error::NoAvailableTeamMembers);
            }

            // Create team members with default roles based on availability
            // In a real implementation, role assignment would be more sophisticated
            let mut team_members = Vec::new();
            
            // Assign first available worker as designer
            if let Some(designer) = available_workers.get(0) {
                team_members.push(TeamMember {
                    account_id: *designer,
                    role: String::from("Designer"),
                    rating: None,
                });
            }
            
            // Assign second available worker as developer
            if let Some(developer) = available_workers.get(1) {
                team_members.push(TeamMember {
                    account_id: *developer,
                    role: String::from("Developer"),
                    rating: None,
                });
            }
            
            // Assign third available worker as tester
            if let Some(tester) = available_workers.get(2) {
                team_members.push(TeamMember {
                    account_id: *tester,
                    role: String::from("Tester"),
                    rating: None,
                });
            }

            if team_members.is_empty() {
                return Err(Error::NoAvailableTeamMembers);
            }

            Ok(team_members)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ink::env::test;

        // Mock calendar contract
        #[ink::test]
        fn assign_coordinator_works() {
            // Setup accounts
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            
            // Set up calendar mock
            let mut mock_calls = BTreeMap::new();
            
            // Mock response for get_available_workers
            let mut available_workers = Vec::new();
            available_workers.push(accounts.charlie);
            
            mock_calls.insert(
                ink::selector_bytes!("get_available_workers"),
                (available_workers.clone(), 0),
            );
            
            ink::env::test::register_chain_extension(|call, _| {
                let mut input = call.input.as_ref();
                let selector = ink::scale::Decode::decode(&mut input).unwrap();
                
                if let Some((output, _)) = mock_calls.get(&selector) {
                    return Ok(ink::env::test::ExtensionResult {
                        return_flags: ink::env::ReturnFlags::default(),
                        data: scale::Encode::encode(output),
                    });
                }
                
                Err(ink::env::Error::ChainExtensionFailed(1))
            });
            
            // Set caller to client
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            
            // Create project
            let calendar_contract = Some(accounts.eve);
            let mut project = Project::new(
                String::from("Website Development"),
                accounts.bob,
                calendar_contract,
            ).unwrap();
            
            // Set caller to DAO address
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            
            // Assign coordinator
            let result = project.assign_coordinator();
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), accounts.charlie);
            assert_eq!(project.coordinator, Some(accounts.charlie));
            assert_eq!(project.status, ProjectStatus::CoordinatorAssigned);
        }

        #[ink::test]
        fn assign_team_works() {
            // Setup accounts
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            
            // Set up calendar mock
            let mut mock_calls = BTreeMap::new();
            
            // Mock response for get_available_workers (for coordinator selection)
            let mut available_workers_coordinator = Vec::new();
            available_workers_coordinator.push(accounts.charlie);
            
            // Mock response for get_available_workers (for team selection)
            let mut available_workers_team = Vec::new();
            available_workers_team.push(accounts.django);
            available_workers_team.push(accounts.frank);
            available_workers_team.push(accounts.eve);
            
            mock_calls.insert(
                ink::selector_bytes!("get_available_workers"),
                (available_workers_team.clone(), 0),
            );
            
            ink::env::test::register_chain_extension(|call, _| {
                let mut input = call.input.as_ref();
                let selector = ink::scale::Decode::decode(&mut input).unwrap();
                
                if let Some((output, _)) = mock_calls.get(&selector) {
                    return Ok(ink::env::test::ExtensionResult {
                        return_flags: ink::env::ReturnFlags::default(),
                        data: scale::Encode::encode(output),
                    });
                }
                
                Err(ink::env::Error::ChainExtensionFailed(1))
            });
            
            // Set caller to client
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            
            // Create project
            let calendar_contract = Some(accounts.eve);
            let mut project = Project::new(
                String::from("Website Development"),
                accounts.bob,
                calendar_contract,
            ).unwrap();
            
            // Manually set coordinator for testing
            project.coordinator = Some(accounts.charlie);
            project.status = ProjectStatus::CoordinatorAssigned;
            
            // Set caller to coordinator
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.charlie);
            
            // Assign team
            let result = project.assign_team();
            assert!(result.is_ok());
            
            let team = result.unwrap();
            assert_eq!(team.len(), 3);
            assert_eq!(team[0].account_id, accounts.django);
            assert_eq!(team[0].role, "Designer");
            assert_eq!(team[1].account_id, accounts.frank);
            assert_eq!(team[1].role, "Developer");
            assert_eq!(team[2].account_id, accounts.eve);
            assert_eq!(team[2].role, "Tester");
            
            assert_eq!(project.status, ProjectStatus::TeamAssigned);
        }

        #[ink::test]
        fn mark_completed_works() {
            // Setup accounts
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            
            // Set caller to client
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            
            // Create project
            let calendar_contract = Some(accounts.eve);
            let mut project = Project::new(
                String::from("Website Development"),
                accounts.bob,
                calendar_contract,
            ).unwrap();
            
            // Manually set coordinator and team for testing
            project.coordinator = Some(accounts.charlie);
            project.team_members = vec![
                TeamMember {
                    account_id: accounts.django,
                    role: String::from("Designer"),
                    rating: None,
                },
                TeamMember {
                    account_id: accounts.frank,
                    role: String::from("Developer"),
                    rating: None,
                },
            ];
            project.status = ProjectStatus::TeamAssigned;
            
            // Set caller to client
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            
            // Mark completed
            let ratings = vec![
                (accounts.django, 8),
                (accounts.frank, 9),
            ];
            
            let result = project.mark_completed(ratings);
            assert!(result.is_ok());
            assert_eq!(project.status, ProjectStatus::Completed);
            
            // Check ratings were applied
            assert_eq!(project.team_members[0].rating, Some(8));
            assert_eq!(project.team_members[1].rating, Some(9));
        }
    }
}