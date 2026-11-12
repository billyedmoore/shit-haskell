{-# LANGUAGE TypeFamilies, DataKinds, KindSignatures #-}
-- A data family whose kind has more arrows than its parameters: instances take extra
-- arguments, eta-reduced away (Cabal's `data family Static (scope :: Scope) :: Type -> Type`).
module Main where

import Data.Kind (Type)

data Scope = User | System

data family Static (scope :: Scope) :: Type -> Type
newtype instance Static 'User fnTy = UserStatic {userPtr :: fnTy}
newtype instance Static 'System fnTy = SystemStatic {systemKey :: Int}

data Command scope arg = Command { action :: Static scope (arg -> String), argument :: arg }

run :: Command 'User arg -> String
run (Command (UserStatic f) a) = f a

toSystem :: Static 'User fnTy -> Static 'System fnTy
toSystem (UserStatic _) = SystemStatic 7

main :: IO ()
main = do
  putStrLn (run (Command (UserStatic show) (42 :: Int)))
  print (systemKey (toSystem (UserStatic not)))
  let cmds = [Command { action = UserStatic (map succ), argument = "abc" }]
  mapM_ (putStrLn . run) cmds
