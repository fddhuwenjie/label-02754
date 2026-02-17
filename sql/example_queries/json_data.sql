-- JSON data handling example
-- Demonstrates MySQL JSON functions

SELECT 
    id,
    username,
    email,
    metadata,
    JSON_EXTRACT(metadata, '$.role') as user_role,
    JSON_EXTRACT(metadata, '$.preferences.theme') as theme_preference
FROM users
WHERE metadata IS NOT NULL;
