import fs from 'fs'

export const token =  fs.readFileSync('./token.txt').toString();
// export const host = "https://games-test.datsteam.dev";
export const host = "http://127.0.0.1:8080";
