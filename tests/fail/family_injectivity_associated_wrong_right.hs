{-# LANGUAGE TypeFamilyDependencies #-}

import Data.Kind (Type)

class C a b where
  type F a = r | r -> b

main :: IO ()
main = pure ()
