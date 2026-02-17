-- Initialize test database with sample tables
-- This script runs automatically when MySQL container starts

SET NAMES utf8mb4;
SET CHARACTER SET utf8mb4;

-- Create sample users table
CREATE TABLE IF NOT EXISTS users (
    id INT AUTO_INCREMENT PRIMARY KEY,
    username VARCHAR(50) NOT NULL UNIQUE,
    email VARCHAR(100) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    status ENUM('active', 'inactive', 'pending') DEFAULT 'pending',
    metadata JSON
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Create sample orders table
CREATE TABLE IF NOT EXISTS orders (
    id INT AUTO_INCREMENT PRIMARY KEY,
    user_id INT NOT NULL,
    order_date DATE NOT NULL,
    total_amount DECIMAL(10, 2) NOT NULL,
    status VARCHAR(20) DEFAULT 'pending',
    notes TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Create sample products table
CREATE TABLE IF NOT EXISTS products (
    id INT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    price DECIMAL(10, 2) NOT NULL,
    stock INT DEFAULT 0,
    category VARCHAR(50),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Insert sample data
INSERT INTO users (username, email, status, metadata) VALUES
('testuser1', 'test1@example.com', 'active', '{"role": "admin", "preferences": {"theme": "dark"}}'),
('testuser2', 'test2@example.com', 'active', '{"role": "user", "preferences": {"theme": "light"}}'),
('testuser3', 'test3@example.com', 'inactive', NULL);

INSERT INTO products (name, description, price, stock, category) VALUES
('Product A', 'Description for product A', 29.99, 100, 'Electronics'),
('Product B', 'Description for product B', 49.99, 50, 'Electronics'),
('Product C', 'Description for product C', 19.99, 200, 'Accessories');

INSERT INTO orders (user_id, order_date, total_amount, status, notes) VALUES
(1, CURDATE(), 79.98, 'completed', 'First order'),
(1, CURDATE() - INTERVAL 1 DAY, 29.99, 'completed', NULL),
(2, CURDATE(), 49.99, 'pending', 'Awaiting payment');

-- Create a sample stored procedure
DELIMITER //
CREATE PROCEDURE IF NOT EXISTS GetUserOrders(IN userId INT)
BEGIN
    SELECT u.username, o.id as order_id, o.order_date, o.total_amount, o.status
    FROM users u
    JOIN orders o ON u.id = o.user_id
    WHERE u.id = userId;
END //
DELIMITER ;

-- Create a sample function
DELIMITER //
CREATE FUNCTION IF NOT EXISTS GetOrderCount(userId INT) RETURNS INT
DETERMINISTIC
READS SQL DATA
BEGIN
    DECLARE orderCount INT;
    SELECT COUNT(*) INTO orderCount FROM orders WHERE user_id = userId;
    RETURN orderCount;
END //
DELIMITER ;
