{-# LANGUAGE RankNTypes, NoImpredicativeTypes, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

bad = Proxy @(Maybe (forall a. a -> a))

main :: IO ()
main = pure ()
