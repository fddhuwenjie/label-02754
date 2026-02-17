-- Daily summary report
-- This file can be configured with an interval in config.toml

SELECT 
    CURDATE() as report_date,
    (SELECT COUNT(*) FROM users) as total_users,
    (SELECT COUNT(*) FROM users WHERE status = 'active') as active_users,
    (SELECT COUNT(*) FROM orders WHERE order_date = CURDATE()) as today_orders,
    (SELECT COALESCE(SUM(total_amount), 0) FROM orders WHERE order_date = CURDATE()) as today_revenue,
    (SELECT COUNT(*) FROM products WHERE stock > 0) as products_in_stock;
