import fs from 'fs';

const basePath = process.env.BASE_PATH || '/net/';
const manifest = fs.readFileSync('public/manifest.template.json', 'utf8');
const sw = fs.readFileSync('public/service-worker.template.js', 'utf8');

fs.writeFileSync('public/manifest.json', manifest.replace(/{{BASE_PATH}}/g, basePath));
fs.writeFileSync('public/service-worker.js', sw.replace(/{{BASE_PATH}}/g, basePath));

console.log(`Assets built with BASE_PATH: ${basePath}`);
