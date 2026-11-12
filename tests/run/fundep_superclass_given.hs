{-# LANGUAGE FunctionalDependencies, FlexibleInstances #-}
-- Fundep improvement through the superclass of a given (regex-base's RegexMaker/RegexOptions).
module Main where

class Opts r c e | r -> c e, c -> r e, e -> r c where
  defC :: c
  defE :: e
  showOpts :: r -> c -> e -> String

class Opts r c e => Maker r c e s | r -> c e, c -> r e, e -> r c where
  make :: s -> r
  make = makeOpts defC defE
  makeOpts :: c -> e -> s -> r

newtype Rx = Rx String deriving Show
newtype CO = CO Int deriving Show
newtype EO = EO Bool deriving Show

instance Opts Rx CO EO where
  defC = CO 7
  defE = EO True
  showOpts (Rx s) (CO n) (EO b) = s ++ "/" ++ show n ++ "/" ++ show b

instance Maker Rx CO EO String where
  makeOpts (CO n) (EO b) s = Rx (s ++ ":" ++ show n ++ ":" ++ show b)

main :: IO ()
main = do
  let r = make "abc" :: Rx
  print r
  putStrLn (showOpts r defC defE)
