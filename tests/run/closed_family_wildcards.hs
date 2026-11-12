{-# LANGUAGE DataKinds, TypeFamilies, TypeOperators, PolyKinds #-}
-- Wildcards in closed family equations (`_ == _ = 'False`) and repeated variables bound to
-- different types (`a == a` at 'User and 'System) let later equations apply.
module Main where

import Data.Proxy (Proxy (..))
import Data.Type.Bool (If)
import Data.Type.Equality (type (==))

data Scope = User | System

type family Pick a b where
  Pick a a = "same"
  Pick _ _ = "different"

class Says (s :: Bool) where says :: Proxy s -> String
instance Says 'True where says _ = "yes"
instance Says 'False where says _ = "no"

x :: If ('User == 'System) Bool Int
x = 3

y :: If ('User == 'User) Bool Int
y = True

main :: IO ()
main = do
  print (x, y)
  putStrLn (says (Proxy :: Proxy ('System == 'User)) ++ " " ++ says (Proxy :: Proxy (Maybe Int == Maybe Int)))
  putStrLn (says (Proxy :: Proxy ([Int] == [Bool])))
