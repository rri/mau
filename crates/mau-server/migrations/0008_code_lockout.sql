-- Wrong second-factor codes (authenticator or recovery) per account, so
-- guessing is limited across sign-ins: 10 wrong within an hour blocks codes
-- for 15 minutes.
ALTER TABLE users ADD COLUMN code_failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN code_failures_since INTEGER;
ALTER TABLE users ADD COLUMN code_locked_until INTEGER;
