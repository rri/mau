-- Catalog changes by change number, so a device's sync finds the works and
-- editions edited since it last asked without reading its whole library.
CREATE INDEX works_seq ON works(seq);
CREATE INDEX editions_seq ON editions(seq);
