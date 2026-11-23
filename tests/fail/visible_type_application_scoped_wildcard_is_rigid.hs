{-# LANGUAGE NamedWildCards, ScopedTypeVariables, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

bad :: forall _item. Proxy _item -> String
bad _ = show (id @_item True)

main :: IO ()
main = pure ()
