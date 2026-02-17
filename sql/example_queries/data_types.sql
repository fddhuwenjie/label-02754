-- Data types demonstration
-- Shows various MySQL data types and their JSON conversion

SELECT 
    -- Integer types
    CAST(123 AS SIGNED) as int_value,
    CAST(9223372036854775807 AS UNSIGNED) as bigint_value,
    
    -- Floating point
    CAST(3.14159 AS DECIMAL(10,5)) as decimal_value,
    CAST(2.71828 AS DOUBLE) as double_value,
    
    -- String types
    'Hello World' as varchar_value,
    
    -- Date/Time types
    CURDATE() as date_value,
    CURTIME() as time_value,
    NOW() as datetime_value,
    CURRENT_TIMESTAMP as timestamp_value,
    
    -- Boolean (MySQL uses TINYINT)
    TRUE as bool_true,
    FALSE as bool_false,
    
    -- NULL
    NULL as null_value;
