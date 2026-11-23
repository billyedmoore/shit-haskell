{-# LANGUAGE NamedWildCards, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

bad = id @(_item, _item) (True, 'x')

main :: IO ()
main = pure ()
