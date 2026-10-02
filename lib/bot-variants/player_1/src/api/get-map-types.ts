export type Vector = {
  x: number;
  y: number;
};

export type Anomaly = {
  effectiveRadius: number;
  id: string;
  radius: number;
  strength: number;
  velocity: Vector;
  x: number;
  y: number;
};

export type Bounty = {
  points: number;
  radius: number;
  x: number;
  y: number;
};

export type Enemy = {
  health: number;
  killBounty: number;
  shieldLeftMs: number;
  status: string;
  velocity: Vector;
  x: number;
  y: number;
};

export type Transport = {
  anomalyAcceleration: Vector;
  attackCooldownMs: number;
  deathCount: number;
  health: number;
  id: string;
  selfAcceleration: Vector;
  shieldCooldownMs: number;
  shieldLeftMs: number;
  status: string;
  velocity: Vector;
  x: number;
  y: number;
};

export type Wanted = {
  health: number;
  killBounty: number;
  shieldLeftMs: number;
  status: string;
  velocity: Vector;
  x: number;
  y: number;
};

export type Desert = {
  errors: string[]
  anomalies: Anomaly[];
  attackCooldownMs: number;
  attackDamage: number;
  attackExplosionRadius: number;
  attackRange: number;
  bounties: Bounty[];
  enemies: Enemy[];
  mapSize: Vector;
  maxAccel: number;
  maxSpeed: number;
  name: string;
  points: number;
  reviveTimeoutSec: number;
  shieldCooldownMs: number;
  shieldTimeMs: number;
  transportRadius: number;
  transports: Transport[];
  wantedList: Wanted[];
};
