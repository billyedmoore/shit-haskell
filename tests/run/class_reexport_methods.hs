-- A class imported bare through a re-exporting module and with its methods from its own module:
-- instances may define the methods (Cabal-syntax's Compat.Prelude re-exports Structured).
module Main where

import Pre
import Str (Structured (..), nominal)
import Data.Proxy

newtype T = T Int
data U = U deriving Show

instance Structured T where structure = nominal
instance Structured U

main :: IO ()
main = mapM_ putStrLn [structure (Proxy :: Proxy T), structure (Proxy :: Proxy U)]
