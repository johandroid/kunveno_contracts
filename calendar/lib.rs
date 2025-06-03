#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod calendar {
    use ink::prelude::vec::Vec;
    use ink::prelude::collections::BTreeMap;
    use ink::storage::Mapping;

    #[ink(storage)]
    pub struct Calendar {
        /// Mapping from worker AccountId to a list of projects they're available for
        worker_availability: Mapping<AccountId, Vec<AccountId>>,
        /// Mapping from project AccountId to a list of available workers
        project_workers: Mapping<AccountId, Vec<AccountId>>,
        /// List of all registered workers
        registered_workers: Vec<AccountId>,
        /// Owner account that has admin privileges
        owner: AccountId,
    }

    #[ink(event)]
    pub struct WorkerRegistered {
        #[ink(topic)]
        worker: AccountId,
    }

    #[ink(event)]
    pub struct AvailabilityUpdated {
        #[ink(topic)]
        worker: AccountId,
        #[ink(topic)]
        project: AccountId,
        is_available: bool,
    }

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        WorkerNotRegistered,
        NotAuthorized,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    impl Calendar {
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                worker_availability: Mapping::default(),
                project_workers: Mapping::default(),
                registered_workers: Vec::new(),
                owner: Self::env().caller(),
            }
        }

        /// Register a new worker
        #[ink(message)]
        pub fn register_worker(&mut self, worker: AccountId) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.owner {
                return Err(Error::NotAuthorized);
            }

            if !self.registered_workers.contains(&worker) {
                self.registered_workers.push(worker);
                self.worker_availability.insert(worker, &Vec::new());
                
                self.env().emit_event(WorkerRegistered { worker });
            }

            Ok(())
        }

        /// Worker sets their availability for a project
        #[ink(message)]
        pub fn set_availability(&mut self, project_id: AccountId, is_available: bool) -> Result<()> {
            let worker = self.env().caller();
            
            if !self.registered_workers.contains(&worker) {
                return Err(Error::WorkerNotRegistered);
            }

            // Update worker's list of available projects
            let mut worker_projects = self.worker_availability.get(worker).unwrap_or_default();
            
            if is_available && !worker_projects.contains(&project_id) {
                worker_projects.push(project_id);
                self.worker_availability.insert(worker, &worker_projects);
                
                // Update project's list of available workers
                let mut available_workers = self.project_workers.get(project_id).unwrap_or_default();
                if !available_workers.contains(&worker) {
                    available_workers.push(worker);
                    self.project_workers.insert(project_id, &available_workers);
                }
            } else if !is_available {
                worker_projects.retain(|&p| p != project_id);
                self.worker_availability.insert(worker, &worker_projects);
                
                // Update project's list of available workers
                let mut available_workers = self.project_workers.get(project_id).unwrap_or_default();
                available_workers.retain(|&w| w != worker);
                self.project_workers.insert(project_id, &available_workers);
            }
            
            self.env().emit_event(AvailabilityUpdated { 
                worker, 
                project: project_id,
                is_available,
            });
            
            Ok(())
        }

        /// Check if a worker is available for a specific project
        #[ink(message)]
        pub fn is_available(&self, account_id: AccountId, project_id: AccountId) -> bool {
            if let Some(projects) = self.worker_availability.get(account_id) {
                projects.contains(&project_id)
            } else {
                false
            }
        }
        
        /// Get all workers available for a specific project
        #[ink(message)]
        pub fn get_available_workers(&self, project_id: AccountId) -> Vec<AccountId> {
            self.project_workers.get(project_id).unwrap_or_default()
        }
        
        /// Admin can register multiple workers at once
        #[ink(message)]
        pub fn register_workers(&mut self, workers: Vec<AccountId>) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.owner {
                return Err(Error::NotAuthorized);
            }

            for worker in workers {
                if !self.registered_workers.contains(&worker) {
                    self.registered_workers.push(worker);
                    self.worker_availability.insert(worker, &Vec::new());
                    
                    self.env().emit_event(WorkerRegistered { worker });
                }
            }

            Ok(())
        }
        
        /// Get all registered workers
        #[ink(message)]
        pub fn get_registered_workers(&self) -> Vec<AccountId> {
            self.registered_workers.clone()
        }
        
        /// Admin can bulk add workers to a project
        #[ink(message)]
        pub fn add_workers_to_project(&mut self, project_id: AccountId, workers: Vec<AccountId>) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.owner {
                return Err(Error::NotAuthorized);
            }

            for worker in &workers {
                if !self.registered_workers.contains(worker) {
                    return Err(Error::WorkerNotRegistered);
                }
                
                // Update worker's projects
                let mut worker_projects = self.worker_availability.get(*worker).unwrap_or_default();
                if !worker_projects.contains(&project_id) {
                    worker_projects.push(project_id);
                    self.worker_availability.insert(*worker, &worker_projects);
                    
                    self.env().emit_event(AvailabilityUpdated { 
                        worker: *worker, 
                        project: project_id,
                        is_available: true,
                    });
                }
            }
            
            // Update project's workers
            let mut project_workers = self.project_workers.get(project_id).unwrap_or_default();
            for worker in workers {
                if !project_workers.contains(&worker) {
                    project_workers.push(worker);
                }
            }
            self.project_workers.insert(project_id, &project_workers);
            
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ink::env::test;

        #[ink::test]
        fn register_worker_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            
            let mut calendar = Calendar::new();
            
            let result = calendar.register_worker(accounts.bob);
            assert!(result.is_ok());
            
            let registered = calendar.get_registered_workers();
            assert_eq!(registered.len(), 1);
            assert_eq!(registered[0], accounts.bob);
        }

        #[ink::test]
        fn set_availability_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            
            // Alice is the owner
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let mut calendar = Calendar::new();
            
            // Register Bob as a worker
            calendar.register_worker(accounts.bob).unwrap();
            
            // Bob sets his availability
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            let project_id = accounts.django;
            
            let result = calendar.set_availability(project_id, true);
            assert!(result.is_ok());
            
            // Check Bob is available for the project
            assert!(calendar.is_available(accounts.bob, project_id));
            
            // Check Bob appears in available workers for project
            let available = calendar.get_available_workers(project_id);
            assert_eq!(available.len(), 1);
            assert_eq!(available[0], accounts.bob);
        }

        #[ink::test]
        fn availability_removal_works() {
            let accounts = test::default_accounts::<ink::env::DefaultEnvironment>();
            
            // Alice is the owner
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            let mut calendar = Calendar::new();
            
            // Register Bob as a worker
            calendar.register_worker(accounts.bob).unwrap();
            
            // Bob sets his availability
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);
            let project_id = accounts.django;
            
            // First make available
            calendar.set_availability(project_id, true).unwrap();
            assert!(calendar.is_available(accounts.bob, project_id));
            
            // Then mark as unavailable
            calendar.set_availability(project_id, false).unwrap();
            assert!(!calendar.is_available(accounts.bob, project_id));
            
            // Check Bob no longer appears in available workers
            let available = calendar.get_available_workers(project_id);
            assert_eq!(available.len(), 0);
        }
    }
}