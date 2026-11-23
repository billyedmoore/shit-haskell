{-# LANGUAGE TypeFamilyDependencies #-}

import Data.Kind (Type)

type family F a = r | r -> b

main :: IO ()
main = pure ()
