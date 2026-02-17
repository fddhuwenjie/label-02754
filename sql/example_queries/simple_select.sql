-- Simple SELECT query example
-- This demonstrates basic query execution

SELECT 
    id,
    username,
    email,
    status,
    created_at
FROM users
WHERE status = 'active'
ORDER BY created_at DESC;
