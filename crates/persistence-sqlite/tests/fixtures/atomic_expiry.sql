-- Synthetic minimal aggregate. Walk the frozen creation and send edges.
INSERT INTO teams(id,name,status,version,created_at,updated_at) VALUES(X'01921389d31174628f80cb5f79ab0001','expiry','ACTIVE',1,1,1);
INSERT INTO roles(id,team_id,display_id,name,status,version,created_at,updated_at) VALUES(X'01921389d31174628f80cb5f79ab0011',X'01921389d31174628f80cb5f79ab0001','target','target','ACTIVE',1,1,1);
INSERT INTO threads(id,team_id,subject,status,version,created_at,updated_at) VALUES(X'01921389d31174628f80cb5f79ab0040',X'01921389d31174628f80cb5f79ab0001','expiry','OPEN',1,1,1);
INSERT INTO messages(id,team_id,thread_id,sender_actor_type,to_role_id,message_type,body,status,version,expires_at,created_at,updated_at) VALUES(X'01921389d31174628f80cb5f79ab0080',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0040','SYSTEM',X'01921389d31174628f80cb5f79ab0011','SYSTEM_DISPATCH','SYNTHETIC_PRIVATE_BODY','DRAFT',1,100,1,1);
UPDATE messages SET status='QUEUED',version=2,updated_at=2 WHERE id=X'01921389d31174628f80cb5f79ab0080';
