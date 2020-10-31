# Chess#0829

This is a discord bot I made that allows one to play chess over discord. Support for other games may be added in future.
The bot's prefix is `c>`.

## Playing a game

* `c>play @Username` Starts a game of chess against @Username. They will have to accept before the game starts.
* `c>accept` Accepts the game, if you were the one who was asked to play. The starting board will be posted.
* `c>decline` Declines the game request, if you were the one who was asked to play. `c>play` can be used again.
* `c>cancel` Cancels a game request, if you were the one who initiated the request. `c>play` can be used again.

When a game has been accepted:
* **Making moves**: To make a move, simply type it out in chat. There's no specific command to make a move
                    The move must be in standard algebraic notation. For example: `e4`, `Nf3`, `dxe5`, `Bxc3`
* **Reposting the board**: If you lost the board image, just run the command `c>board` to get it back