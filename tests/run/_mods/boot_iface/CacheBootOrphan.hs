{-# LANGUAGE StandaloneDeriving, GeneralizedNewtypeDeriving, DerivingStrategies #-}
module CacheBootOrphan where
import {-# SOURCE #-} CacheBootOrphanType (N(..))
deriving newtype instance Num N
