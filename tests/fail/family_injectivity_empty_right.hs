{-# LANGUAGE TypeFamilyDependencies #-}

import Data.Kind (Type)

type family F a = r | r ->

main :: IO ()
main = pure ()
