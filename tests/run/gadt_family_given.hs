{-# LANGUAGE DataKinds, GADTs, TypeFamilies #-}
import Data.Proxy

data SBool (b :: Bool) where
  STrue :: SBool 'True
  SFalse :: SBool 'False

type family Choose (b :: Bool) a c where
  Choose 'True a c = a
  Choose 'False a c = c

type family Flip (b :: Bool) :: Bool
type instance Flip 'True = 'False
type instance Flip 'False = 'True

use :: Proxy b -> SBool (Flip b) -> Choose (Flip b) Int String -> String
use _ STrue x = show (x + 1)
use _ SFalse s = s ++ "!"

main :: IO ()
main = do
  putStrLn (use (Proxy :: Proxy 'False) STrue 41)
  putStrLn (use (Proxy :: Proxy 'True) SFalse "ok")
