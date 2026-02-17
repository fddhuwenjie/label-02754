-- Multiple result sets example
-- This query returns multiple result sets

/* 
 * First result set: User statistics
 * Shows count of users by status
 */
SELECT 
    status,
    COUNT(*) as user_count
FROM users
GROUP BY status;

-- Second result set: Order statistics
SELECT 
    status,
    COUNT(*) as order_count,
    SUM(total_amount) as total_revenue
FROM orders
GROUP BY status;

/* Third result set: Product inventory */
SELECT 
    category,
    COUNT(*) as product_count,
    SUM(stock) as total_stock,
    AVG(price) as avg_price
FROM products
GROUP BY category;
