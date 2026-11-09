module GHC.Stack (HasCallStack) where

import Base

-- Call stacks are not tracked; the constraint is accepted and always satisfied.
class HasCallStack

instance HasCallStack
