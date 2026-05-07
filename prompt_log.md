Refactor the dj_controller into the dispatcher
- I want the dispatcher to internally hold and update the state of each dj controller in the network
- if a foreign node sends a packet which implies, that it is a dj controller
  (Status, Metrics, BeatGrid, Cue, SmallWaveform, BigWaveform, Mixer, ArtworkFile, Time)
the dispatcher should add set an optional field dj_controller in the foreign node struct to the DjController struct
- use the code which is currently DjControllerView for DjController
- no active requests for data should be made by the djcontroller without the user having requested it
- the user shall be able to read the state from an async buffer(i.e. a tripple buffer) and be able
  to activeley request more information about the controller. the requested data may be hold in the dispatcher's 
  DjController internally
- for convenience, the user shall not be required to handle the async buffer, but be exposed a DjControllerView
  (whiches code shall not be reused from the current DjControllerView)
- the future DjControllerView shall hand live information to the user via functions like
  - get_layers -> &Vec<LayerSnapshot>
  - get_mixer -> &MixerSnapshot
- data which must be requested from a controller shall be retrieved by the user via an async function call which will time out
  no information is provided by the foreign node: 
    - request_SmallWaveform
    - request_BigWaveform
    - request_ArtworkFile
    - ArtworkFile
- Communication between the dispatcher threads and the dj controller update thread shall still happen as channels
- the different applications can be removed and the entire dispatcher code can instead have just a single node id
- a list of all current active nodes shall be retrievable from the TCNetClient. This data transfer must also happen via
  an async buffer

Test fixture
- write a little test suite which sends test packets to the client
- for an initial test I just want to test sending a prototypical play session from a pioneer cdj setup
- a few tracks shall be loaded and the mixer shall be configured to some state which could occur in the real world
- the test shall send the necessary opt in packages at an intervall + the mixer and status information
- it shall then be asserted if that data is correctly present in the user facing djcontroller view

CDJ3000 + DJMa9 test master client simulation
- I want a gui which looks like the interface of a dj deck with 2 cdj3000 plus a dma9 next to each other
- it shall internally use the existing state structs which allready exist in this repo to store its own state
- it shall implement all relevant functions of the dj system which could alter this state i.e. but not limited to
  - playback
  - loading waveforms
  - a dedicated folder should be used as a "virtual usb stick" which contains track available on the system
  - ...
- despite the real cdjs being different devices which are connected by their own network protocol, this is level of
- simulation is not required and the deck can live in a shared struct CDJDeck. 
- use the manuals of both devices (cd3000.pdf + djma9000.pdf) to figure out their layouts and functionality and research
online to find information about their inner workings
- the gui should be functional and the deck should also actually do the audio replay
- use egui + winit + wgpu to implent the gui
- broadcast
  via tcnet. however for sending packet. build a similar convenient interface like with the dj views which allready exist, so that users of the tcnet library do not   
  have to mess with channels, but can just a simple synchronous function which handles sending the relevant updates via the dispatcher. make sure to see the tcnet spec
  on how status updates have to be sent to the network

Now make the simulator client completely ready to be used in combination with a viewer
- make sure that the simulation client can answer tcnet requests as described in the spec pdf
- artwork file should give the file thumbnail
- small and big waveform
- Metrics Data
- metadata
- beat grid info
- cue data info
- mixer data (is send anyway based on time, but the request should still be answered)

- make sure that the data packets which are not requested, but are automatically sent, should never be sent to master nodes
(see distinction on roles in the tncet network from the spec doc).
- write a test which requests all these data types from a second process via localhost, and validate
- read the spec again and verify that the active controller simulator adheres to it. if there a conflicts between the spec
and my instruction or the current implementation inform me and ask how to resolve!

Write a tcnet dj controller viewer [[bin]]
- It shall work like the viewer in ShowKontrol; see layout.png for that
- A TcnetClient shall be created and as soon as there is a user facing djcontrollerview the gui shall go from "No controller detected"
into the viewer as described

- Two-column layout in header area (Deck 1+3 left, Deck 2+4 right)
- Four stacked full-width waveform rows below
- Dark/black background throughout
- Synchronized vertical playhead across all waveform lanes
- Here's a spec of the functional items visible in this DJ software interface:

**Per-Deck Header Strip for each of the 4 decks
- Deck number badge
- Track title display
- Artist name sub-label
- Elapsed/remaining time counter (MM:SS.milliseconds format)
- BPM display box (e.g. 137.50 BPM)
- Mini waveform overview (full-track thumbnail showing position)
- Playhead position indicator
- EQ/mixer icon button (the ⇌ arrows)
- Play/Pause button (‖ symbol)

**Main Waveform Lanes (×4 decks)**
- Full-width scrolling waveform per deck
- White/grey base layer beneath color waveform
- Beat grid overlay
- Waveform transitions between two track segments (visible color/density change mid-lane)
- Settings gear icon (⚙) per lane
- Tempo offset display (e.g. "+ 0.00% TEMPO") per lane
- Play/Pause button (‖) per lane
- Deck label (DECK 1–4) on left side of each lane

**Global / Layout**
- Four stacked full-width waveform rows below
- Synchronized vertical playhead across all waveform lanes


I want it to be possible to have two tcnetrs based nodes on one computer. dont use shared port binds, but instead if a port cannot be bound, just try     
with another for all 4 ports respectively. and update the node config if its the unicastport. other nodes will still be able to receive messages if non standard ports are used, if they themself listen on the default      
broadcasting ports 60000, 60001 and 60002. only communication between two nodes which dont use standard broadcasing ports for listening. All nodes independent if they bound canonical ports or not shall however still      
target their packages to the canonical broadcasting ports and the announced unicast port. this behavior is also partly described in the spec pdf 