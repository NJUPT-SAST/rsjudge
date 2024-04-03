/// Generate functions to get user instances.
macro_rules! users {
    ($($vis:vis fn $id:ident() => $name:literal);* $(;)?) => {
        $(
            #[doc = concat!("Get an instance of user `", $name, "`.")]
            ///
            /// # Errors
            /// Returns an error if the user is not found.
            pub fn $id() -> Result<&'static User> {
                static INNER: OnceLock<Option<User>> = OnceLock::new();
                INNER
                    .get_or_init(|| get_user_by_name($name))
                    .as_ref()
                    .ok_or_else(|| Error::UserNotFound { username: $name })
            }
        )*
    };
}
