-- CTE (Common Table Expression) example
-- Demonstrates WITH clause support

WITH user_order_summary AS (
    SELECT 
        u.id as user_id,
        u.username,
        COUNT(o.id) as order_count,
        COALESCE(SUM(o.total_amount), 0) as total_spent
    FROM users u
    LEFT JOIN orders o ON u.id = o.user_id
    GROUP BY u.id, u.username
),
high_value_users AS (
    SELECT *
    FROM user_order_summary
    WHERE total_spent > 50
)
SELECT 
    user_id,
    username,
    order_count,
    total_spent,
    CASE 
        WHEN total_spent > 100 THEN 'VIP'
        WHEN total_spent > 50 THEN 'Regular'
        ELSE 'New'
    END as customer_tier
FROM high_value_users
ORDER BY total_spent DESC;
