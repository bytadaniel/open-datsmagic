import { CoordinateMap } from "./CoordinateMap";
import { Player } from "./player/Player";

export const MAP_SIZE = 100;
export const MAP = new CoordinateMap(MAP_SIZE, MAP_SIZE, 100);

export const PLAYER_FOV_OF_MAP = 1;
export const PLAYER = new Player(0, 0);

