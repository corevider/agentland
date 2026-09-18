# Project engines and reference folders

Both **Start** and **Repositories → Open a project** offer project engine choices
and reference folders. Existing projects can be edited under **Repositories →
project settings**. Settings are stored with the project on this machine and
survive reopening the folder and restarting Agentland.

Select one or more engines to limit new hires and engine switches for that
project. Selecting none inherits the global hiring policy. Project settings
cannot reopen an engine disabled globally. The default commander engine is
optional; automatic selection stays within the project's allowed, installed
engines that support crew tools. An explicitly selected unavailable engine
produces an error instead of silently switching providers. These are project
choices; a workspace chief still uses the workspace's engine choice. Existing
agents are not stopped or migrated when project settings change.

The Crew picker follows project engine choices. The core also checks them for
new hires, commander creation, engine switches and hand-opened CLIs using crew
authority. A person's independent CLI remains their own. Only a person may
save project settings; an agent cannot relax the policy through the settings
endpoint or by re-registering the project with different options.

Each reference has an absolute folder path and an optional description. Use
**browse…**, enter the description, then **add reference**. References need not
be Git repositories. Up to 32 existing directories can be attached. Duplicate,
relative, missing and non-directory paths are rejected. Removing a reference
only removes its association with the project.

The common instructions given to every engine include the reference paths and
descriptions. Running agents receive updates in their next composed brief;
fresh or resumed sessions receive the current configuration with their shared
instructions. Folder contents are not automatically copied, indexed or sent
to a model. The crew is instructed to inspect only task-relevant background
material and obtain separate authorization before modifying, executing,
copying or uploading its contents. Reference registration adds no native
filesystem permission and is not an operating-system read-only mount.

Opening, cloning, starting, removing and configuring a project now notify the
other visible panels and application windows. Workspace membership, Board,
Crew, Files & Git and the sidebar re-read their data without **Reload
interface**. Visible panels also poll as a fallback for changes made outside
that window; returning to a hidden window refreshes it.

Project settings also include independent commit, push, PR and merge switches,
triggers and commit classification templates. See [Git workflows](delivery-workflows.md).
