{-# LANGUAGE GADTs, TypeFamilies, DataKinds, KindSignatures, RankNTypes #-}
-- GADT record constructors with a forall and a context before the fields (Cabal's
-- SetupHooks RuleCommands).
module Main where

import Data.Kind (Type)

data Scope = User | System

data Cmd (s :: Scope) where
  Static
    :: forall arg s
     . (Show arg, Num arg)
    => { staticArg :: !arg
         -- ^ The argument.
       , staticName :: String
       }
    -> Cmd s
  Plain :: { plainName :: String } -> Cmd 'User
  Pos :: forall a. Show a => a -> Cmd 'System

describe :: Cmd s -> String
describe (Static a n) = n ++ "=" ++ show (a + 1)
describe (Plain n) = n
describe (Pos a) = show a

main :: IO ()
main = do
  mapM_ (putStrLn . describe) [Static (41 :: Int) "answer" :: Cmd 'User, Plain "p"]
  putStrLn (describe (Pos [True]))
  putStrLn (staticName (Static (1.5 :: Double) "half" :: Cmd 'System))
