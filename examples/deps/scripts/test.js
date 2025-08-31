#!/usr/bin/env node
// Example test runner script for CCPM
// This script demonstrates how to use JavaScript scripts as dependencies

console.log('🧪 Running tests...');

const tests = [
    { name: 'Unit tests', command: 'cargo test --lib' },
    { name: 'Integration tests', command: 'cargo test --test' },
    { name: 'Documentation tests', command: 'cargo test --doc' }
];

let allPassed = true;

for (const test of tests) {
    console.log(`  - Running ${test.name}...`);
    
    // Simulate test execution
    const passed = Math.random() > 0.1; // 90% success rate for demo
    
    if (passed) {
        console.log(`    ✅ ${test.name} passed`);
    } else {
        console.log(`    ❌ ${test.name} failed`);
        allPassed = false;
    }
}

if (allPassed) {
    console.log('✨ All tests passed!');
    process.exit(0);
} else {
    console.log('💥 Some tests failed');
    process.exit(1);
}