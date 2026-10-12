-- DEV-02-06: Artifact and Reference tables.
-- IDs use the 16-byte UUIDv7 BLOB representation frozen in DATA_CONVENTIONS.md;
-- timestamps are UTC Unix milliseconds.

CREATE TABLE artifacts (
    id BLOB PRIMARY KEY NOT NULL
        CHECK (
            typeof(id) = 'blob'
            AND length(id) = 16
            AND substr(hex(id), 13, 1) = '7'
            AND substr(hex(id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    team_id BLOB NOT NULL
        CHECK (
            typeof(team_id) = 'blob'
            AND length(team_id) = 16
            AND substr(hex(team_id), 13, 1) = '7'
            AND substr(hex(team_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    task_id BLOB NOT NULL
        CHECK (
            typeof(task_id) = 'blob'
            AND length(task_id) = 16
            AND substr(hex(task_id), 13, 1) = '7'
            AND substr(hex(task_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    run_id BLOB NOT NULL
        CHECK (
            typeof(run_id) = 'blob'
            AND length(run_id) = 16
            AND substr(hex(run_id), 13, 1) = '7'
            AND substr(hex(run_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    creator_role_id BLOB NOT NULL
        CHECK (
            typeof(creator_role_id) = 'blob'
            AND length(creator_role_id) = 16
            AND substr(hex(creator_role_id), 13, 1) = '7'
            AND substr(hex(creator_role_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    artifact_type TEXT NOT NULL CHECK (artifact_type IN ('DOCUMENT', 'CODE', 'TEST_REPORT', 'BUILD_LOG', 'DECISION_RECORD', 'QUESTION', 'OTHER')),
    uri TEXT NOT NULL CHECK (length(trim(uri)) > 0),
    checksum_sha256 TEXT CHECK (checksum_sha256 IS NULL OR (length(checksum_sha256) = 64 AND hex(unhex(checksum_sha256)) = upper(checksum_sha256))),
    git_commit_sha TEXT CHECK (git_commit_sha IS NULL OR (length(git_commit_sha) >= 7 AND length(git_commit_sha) <= 40 AND hex(unhex(git_commit_sha)) = upper(git_commit_sha))),
    visibility TEXT NOT NULL CHECK (visibility IN ('PUBLIC', 'TEAM_INTERNAL', 'ROLE_RESTRICTED')),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    CONSTRAINT artifacts_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT artifacts_task_id_foreign_key
        FOREIGN KEY (task_id) REFERENCES tasks (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT artifacts_run_id_foreign_key
        FOREIGN KEY (run_id) REFERENCES runs (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT artifacts_creator_role_id_foreign_key
        FOREIGN KEY (creator_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE INDEX artifacts_team_id ON artifacts (team_id);
CREATE INDEX artifacts_task_id ON artifacts (task_id);
CREATE INDEX artifacts_run_id ON artifacts (run_id);
CREATE INDEX artifacts_creator_role_id ON artifacts (creator_role_id);

CREATE TABLE "references" (
    id BLOB PRIMARY KEY NOT NULL
        CHECK (
            typeof(id) = 'blob'
            AND length(id) = 16
            AND substr(hex(id), 13, 1) = '7'
            AND substr(hex(id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    team_id BLOB NOT NULL
        CHECK (
            typeof(team_id) = 'blob'
            AND length(team_id) = 16
            AND substr(hex(team_id), 13, 1) = '7'
            AND substr(hex(team_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    subject_type TEXT NOT NULL CHECK (subject_type IN ('TASK', 'RUN', 'ARTIFACT', 'MESSAGE')),
    subject_id BLOB NOT NULL
        CHECK (
            typeof(subject_id) = 'blob'
            AND length(subject_id) = 16
            AND substr(hex(subject_id), 13, 1) = '7'
            AND substr(hex(subject_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    reference_kind TEXT NOT NULL CHECK (reference_kind IN ('WORKSPACE_PATH', 'HEADING_ANCHOR', 'COMMIT', 'EXTERNAL_URL', 'ARTIFACT_LINK')),
    target_uri TEXT NOT NULL CHECK (length(trim(target_uri)) > 0),
    validation_status TEXT NOT NULL CHECK (validation_status IN ('VALID', 'INVALID', 'UNVERIFIED', 'STALE')),
    checksum_sha256 TEXT CHECK (checksum_sha256 IS NULL OR (length(checksum_sha256) = 64 AND hex(unhex(checksum_sha256)) = upper(checksum_sha256))),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    validated_at INTEGER CHECK (validated_at IS NULL OR typeof(validated_at) = 'integer'),
    CONSTRAINT references_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE INDEX references_team_id ON "references" (team_id);
CREATE INDEX references_subject ON "references" (subject_type, subject_id);
