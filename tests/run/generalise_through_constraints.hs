{-# LANGUAGE FlexibleContexts, NoMonomorphismRestriction #-}
-- A variable that only a constraint mentions (`m` of `MonadReader (Env m) n`, determined by
-- the functional dependency) is generalised with the type's (ShellCheck's parser helpers).
module Main where

import Control.Monad.Reader

data Env m = Env { envName :: String, envAction :: m () }

name = asks envName

shout = do
  n <- name
  return (n ++ "!")

main :: IO ()
main = do
  putStrLn (runReader shout (Env "maybe" (Just ())))
  putStrLn (runReader shout (Env "list" [(), ()]))
  r <- runReaderT shout (Env "io" (putStr ""))
  putStrLn r
