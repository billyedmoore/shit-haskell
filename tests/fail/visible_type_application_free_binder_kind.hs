{-# LANGUAGE ImpredicativeTypes, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

bad = Proxy @(forall (x :: missing). Proxy x)

main :: IO ()
main = pure ()
