-- A user's own Google Books API key, used for their catalog searches and
-- imports. NULL means they haven't added one.
ALTER TABLE users ADD COLUMN google_books_key TEXT;
