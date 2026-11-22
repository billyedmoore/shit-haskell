{-# LANGUAGE ConstraintKinds #-}

import Data.Kind (Constraint)

class c a => A (c :: * -> Constraint) a

main = pure ()
