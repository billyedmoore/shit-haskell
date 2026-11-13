-- Haskell98 (Cabal's default language) allows a do block at the column of the let block that
-- holds it (NondecreasingIndentation), as ShellCheck's Analytics writes.
module Main where

import Control.Monad (guard)
import Data.Char (isDigit)

literalEquals :: [String] -> [String]
literalEquals parts =
    let found = do
        str <- parts
        let (before, after) = break ('=' ==) str
        guard $ all isDigit before && not (null after)
        return before
    in found

main :: IO ()
main = print (literalEquals ["1=a", "x=b", "22=", "3"])
