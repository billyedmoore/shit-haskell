{-# LANGUAGE TypeFamilyDependencies #-}

import Data.Kind (Type)

data family F a = r | r -> a

main :: IO ()
main = pure ()
